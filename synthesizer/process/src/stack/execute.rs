// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;

impl<N: Network> StackExecute<N> for Stack<N> {
    /// Executes a program closure on the given inputs.
    ///
    /// # Errors
    /// This method will halt if the given inputs are not the same length as the input statements.
    #[inline]
    fn execute_closure<A: circuit::Aleo<Network = N>>(
        &self,
        closure: &Closure<N>,
        inputs: &[circuit::Value<A>],
        call_stack: CallStack<N>,
        caller: circuit::Address<A>,
        tvk: circuit::Field<A>,
    ) -> Result<Vec<circuit::Value<A>>> {
        let timer = timer!("Stack::execute_closure");

        // Ensure the call stack is not `Evaluate`.
        ensure!(!matches!(call_stack, CallStack::Evaluate(..)), "Illegal operation: cannot evaluate in execute mode");

        // Ensure the number of inputs matches the number of input statements.
        if closure.inputs().len() != inputs.len() {
            bail!("Expected {} inputs, found {}", closure.inputs().len(), inputs.len())
        }
        lap!(timer, "Check the number of inputs");

        // Retrieve the number of public variables in the circuit.
        let num_public = A::num_public();

        // Initialize the registers.
        let mut registers = Registers::new(call_stack, self.get_register_types(closure.name())?.clone());
        // Set the transition caller, as a circuit.
        registers.set_caller_circuit(caller);
        // Set the transition view key, as a circuit.
        registers.set_tvk_circuit(tvk);
        lap!(timer, "Initialize the registers");

        // Store the inputs.
        closure.inputs().iter().map(|i| i.register()).zip_eq(inputs).try_for_each(|(register, input)| {
            // If the circuit is in execute mode, then store the console input.
            if let CallStack::Execute(..) = registers.call_stack() {
                use circuit::Eject;
                // Assign the console input to the register.
                registers.store(self, register, input.eject_value())?;
            }
            // Assign the circuit input to the register.
            registers.store_circuit(self, register, input.clone())
        })?;
        lap!(timer, "Store the inputs");

        // Execute the instructions.
        for instruction in closure.instructions() {
            // If the circuit is in execute mode, then evaluate the instructions.
            if let CallStack::Execute(..) = registers.call_stack() {
                // If the evaluation fails, bail and return the error.
                if let Err(error) = instruction.evaluate(self, &mut registers) {
                    bail!("Failed to evaluate instruction ({instruction}): {error}");
                }
            }
            // Execute the instruction.
            instruction.execute(self, &mut registers)?;
        }
        lap!(timer, "Execute the instructions");

        // Ensure the number of public variables remains the same.
        ensure!(A::num_public() == num_public, "Illegal closure operation: instructions injected public variables");

        use circuit::Inject;

        // Load the outputs.
        let outputs = closure
            .outputs()
            .iter()
            .map(|output| {
                match output.operand() {
                    // If the operand is a literal, use the literal directly.
                    Operand::Literal(literal) => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::new(circuit::Mode::Constant, literal.clone()),
                    ))),
                    // If the operand is a register, retrieve the stack value from the register.
                    Operand::Register(register) => registers.load_circuit(self, &Operand::Register(register.clone())),
                    // If the operand is the program ID, convert the program ID into an address.
                    Operand::ProgramID(program_id) => {
                        Ok(circuit::Value::Plaintext(circuit::Plaintext::from(circuit::Literal::Address(
                            circuit::Address::new(circuit::Mode::Constant, program_id.to_address()?),
                        ))))
                    }
                    // If the operand is the caller, retrieve the caller from the registers.
                    Operand::Caller => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.caller_circuit()?),
                    ))),
                    // If the operand is the block height, throw an error.
                    Operand::BlockHeight => {
                        bail!("Illegal operation: cannot retrieve the block height in a closure scope")
                    }
                }
            })
            .collect();
        lap!(timer, "Load the outputs");

        finish!(timer);
        outputs
    }

    /// Executes a program function on the given inputs.
    ///
    /// Note: To execute a transition, do **not** call this method. Instead, call `Process::execute`.
    ///
    /// # Errors
    /// This method will halt if the given inputs are not the same length as the input statements.
    #[inline]
    fn execute_function<A: circuit::Aleo<Network = N>>(&self, mut call_stack: CallStack<N>) -> Result<Response<N>> {
        let timer = timer!("Stack::execute_function");

        // Ensure the call stack is not `Evaluate`.
        ensure!(!matches!(call_stack, CallStack::Evaluate(..)), "Illegal operation: cannot evaluate in execute mode");

        // Ensure the circuit environment is clean.
        A::reset();

        // Retrieve the next request.
        let console_request = call_stack.pop()?;

        // Ensure the network ID matches.
        ensure!(
            **console_request.network_id() == N::ID,
            "Network ID mismatch. Expected {}, but found {}",
            N::ID,
            console_request.network_id()
        );

        // Retrieve the function from the program.
        let function = self.get_function(console_request.function_name())?;
        // Retrieve the number of inputs.
        let num_inputs = function.inputs().len();
        // Ensure the number of inputs matches the number of input statements.
        if num_inputs != console_request.inputs().len() {
            bail!("Expected {num_inputs} inputs, found {}", console_request.inputs().len())
        }
        // Retrieve the input types.
        let input_types = function.input_types();
        // Retrieve the output types.
        let output_types = function.output_types();
        lap!(timer, "Retrieve the input and output types");

        // Ensure the inputs match their expected types.
        console_request.inputs().iter().zip_eq(&input_types).try_for_each(|(input, input_type)| {
            // Ensure the input matches the input type in the function.
            self.matches_value_type(input, input_type)
        })?;
        lap!(timer, "Verify the input types");

        // Ensure the request is well-formed.
        ensure!(console_request.verify(&input_types), "Request is invalid");
        lap!(timer, "Verify the request");

        // Initialize the registers.
        let mut registers = Registers::new(call_stack, self.get_register_types(function.name())?.clone());

        use circuit::{Eject, Inject};

        // Inject the transition public key `tpk` as `Mode::Public`.
        let tpk = circuit::Group::<A>::new(circuit::Mode::Public, console_request.to_tpk());
        // Inject the request as `Mode::Private`.
        let request = circuit::Request::new(circuit::Mode::Private, console_request.clone());
        // Ensure the request has a valid signature, inputs, and transition view key.
        A::assert(request.verify(&input_types, &tpk));

        // Set the transition caller.
        registers.set_caller(*console_request.caller());
        // Set the transition caller, as a circuit.
        registers.set_caller_circuit(request.caller().clone());

        // Set the transition view key.
        registers.set_tvk(*console_request.tvk());
        // Set the transition view key, as a circuit.
        registers.set_tvk_circuit(request.tvk().clone());

        lap!(timer, "Initialize the registers");

        #[cfg(debug_assertions)]
        Self::log_circuit::<A, _>("Request");

        // Retrieve the number of constraints for verifying the request in the circuit.
        let num_request_constraints = A::num_constraints();

        // Retrieve the number of public variables in the circuit.
        let num_public = A::num_public();

        // Store the inputs.
        function.inputs().iter().map(|i| i.register()).zip_eq(request.inputs()).try_for_each(|(register, input)| {
            // If the circuit is in execute mode, then store the console input.
            if let CallStack::Execute(..) = registers.call_stack() {
                // Assign the console input to the register.
                registers.store(self, register, input.eject_value())?;
            }
            // Assign the circuit input to the register.
            registers.store_circuit(self, register, input.clone())
        })?;
        lap!(timer, "Store the inputs");

        // Initialize a tracker to determine if there are any function calls.
        let mut contains_function_call = false;

        // Execute the instructions.
        for instruction in function.instructions() {
            // If the circuit is in execute mode, then evaluate the instructions.
            if let CallStack::Execute(..) = registers.call_stack() {
                // Evaluate the instruction.
                let result = match instruction {
                    // If the instruction is a `call` instruction, we need to handle it separately.
                    Instruction::Call(call) => CallTrait::evaluate(call, self, &mut registers),
                    // Otherwise, evaluate the instruction normally.
                    _ => instruction.evaluate(self, &mut registers),
                };
                // If the evaluation fails, bail and return the error.
                if let Err(error) = result {
                    bail!("Failed to evaluate instruction ({instruction}): {error}");
                }
            }

            // Execute the instruction.
            let result = match instruction {
                // If the instruction is a `call` instruction, we need to handle it separately.
                Instruction::Call(call) => CallTrait::execute(call, self, &mut registers),
                // Otherwise, execute the instruction normally.
                _ => instruction.execute(self, &mut registers),
            };
            // If the execution fails, bail and return the error.
            if let Err(error) = result {
                bail!("Failed to execute instruction ({instruction}): {error}");
            }

            // If the instruction was a function call, then set the tracker to `true`.
            if let Instruction::Call(call) = instruction {
                // Check if the call is a function call.
                if call.is_function_call(self)? {
                    contains_function_call = true;
                }
            }
        }
        lap!(timer, "Execute the instructions");

        // Load the outputs.
        let output_operands = &function.outputs().iter().map(|output| output.operand()).collect::<Vec<_>>();
        let outputs = output_operands
            .iter()
            .map(|operand| {
                match operand {
                    // If the operand is a literal, use the literal directly.
                    Operand::Literal(literal) => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::new(circuit::Mode::Constant, literal.clone()),
                    ))),
                    // If the operand is a register, retrieve the stack value from the register.
                    Operand::Register(register) => registers.load_circuit(self, &Operand::Register(register.clone())),
                    // If the operand is the program ID, convert the program ID into an address.
                    Operand::ProgramID(program_id) => {
                        Ok(circuit::Value::Plaintext(circuit::Plaintext::from(circuit::Literal::Address(
                            circuit::Address::new(circuit::Mode::Constant, program_id.to_address()?),
                        ))))
                    }
                    // If the operand is the caller, retrieve the caller from the registers.
                    Operand::Caller => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.caller_circuit()?),
                    ))),
                    // If the operand is the block height, throw an error.
                    Operand::BlockHeight => {
                        bail!("Illegal operation: cannot retrieve the block height in a function scope")
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;
        lap!(timer, "Load the outputs");

        // Map the output operands into registers.
        let output_registers = output_operands
            .iter()
            .map(|operand| match operand {
                Operand::Register(register) => Some(register.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        #[cfg(debug_assertions)]
        Self::log_circuit::<A, _>(format!("Function '{}()'", function.name()));

        // Retrieve the number of constraints for executing the function in the circuit.
        let num_function_constraints = A::num_constraints().saturating_sub(num_request_constraints);

        // If the function does not contain function calls, ensure no new public variables were injected.
        if !contains_function_call {
            // Ensure the number of public variables remains the same.
            ensure!(A::num_public() == num_public, "Instructions in function injected public variables");
        }

        // Construct the response.
        let response = circuit::Response::from_outputs(
            request.network_id(),
            request.program_id(),
            request.function_name(),
            num_inputs,
            request.tvk(),
            request.tcm(),
            outputs,
            &output_types,
            &output_registers,
        );
        lap!(timer, "Construct the response");

        #[cfg(debug_assertions)]
        Self::log_circuit::<A, _>("Response");

        // Retrieve the number of constraints for verifying the response in the circuit.
        let num_response_constraints =
            A::num_constraints().saturating_sub(num_request_constraints).saturating_sub(num_function_constraints);

        // If the circuit is in `Execute` mode, then prepare the 'finalize' scope if it exists.
        let finalize = if matches!(registers.call_stack(), CallStack::Synthesize(..))
            || matches!(registers.call_stack(), CallStack::CheckDeployment(..))
            || matches!(registers.call_stack(), CallStack::Execute(..))
        {
            // If this function has the finalize command, then construct the finalize inputs.
            if let Some(command) = function.finalize_command() {
                use circuit::ToBits;

                // Ensure the number of inputs is within bounds.
                ensure!(
                    command.operands().len() <= N::MAX_INPUTS,
                    "The 'finalize' command contains too many operands. The maximum number of inputs is {}.",
                    N::MAX_INPUTS
                );

                // Initialize a vector for the (console) finalize inputs.
                let mut console_finalize_inputs = Vec::with_capacity(command.operands().len());
                // Initialize a vector for the (circuit) finalize input bits.
                let mut circuit_finalize_input_bits = Vec::with_capacity(command.operands().len());

                // Retrieve the finalize inputs.
                for operand in command.operands() {
                    // Retrieve the finalize input.
                    let value = registers.load_circuit(self, operand)?;
                    // Ensure the value is a literal or a struct.
                    // See `RegisterTypes::initialize_function_types()` for the same set of checks.
                    match value {
                        circuit::Value::Plaintext(circuit::Plaintext::Literal(..)) => (),
                        circuit::Value::Plaintext(circuit::Plaintext::Struct(..)) => (),
                        circuit::Value::Record(..) => {
                            bail!(
                                "'{}/{}' attempts to pass a 'record' into 'finalize'",
                                self.program_id(),
                                function.name()
                            );
                        }
                    }

                    // Store the (console) finalize input.
                    console_finalize_inputs.push(value.eject_value());
                    // Store the (circuit) finalize input bits.
                    circuit_finalize_input_bits.extend(value.to_bits_le());
                }

                // Compute the finalize inputs checksum.
                let finalize_checksum = A::hash_bhp1024(&circuit_finalize_input_bits);
                // Inject the finalize inputs checksum as `Mode::Public`.
                let circuit_checksum = circuit::Field::<A>::new(circuit::Mode::Public, finalize_checksum.eject_value());
                // Enforce the injected checksum matches the original checksum.
                A::assert_eq(circuit_checksum, finalize_checksum);

                #[cfg(debug_assertions)]
                Self::log_circuit::<A, _>("Finalize");

                lap!(timer, "Construct the finalize inputs");

                // Return the (console) finalize inputs.
                Some(console_finalize_inputs)
            } else {
                None
            }
        } else {
            None
        };

        #[cfg(debug_assertions)]
        Self::log_circuit::<A, _>("Complete");

        // Eject the response.
        let response = response.eject_value();

        // Ensure the outputs matches the expected value types.
        response.outputs().iter().zip_eq(&output_types).try_for_each(|(output, output_type)| {
            // Ensure the output matches its expected type.
            self.matches_value_type(output, output_type)
        })?;

        // If the circuit is in `Execute` mode, then ensure the circuit is satisfied.
        if let CallStack::Execute(..) = registers.call_stack() {
            // If the circuit is empty or not satisfied, then throw an error.
            ensure!(
                A::num_constraints() > 0 && A::is_satisfied(),
                "'{}/{}' is not satisfied on the given inputs ({} constraints).",
                self.program.id(),
                function.name(),
                A::num_constraints()
            );
        }

        // Eject the circuit assignment and reset the circuit.
        let assignment = A::eject_assignment_and_reset();

        // If the circuit is in `Synthesize` or `Execute` mode, synthesize the circuit key, if it does not exist.
        if matches!(registers.call_stack(), CallStack::Synthesize(..))
            || matches!(registers.call_stack(), CallStack::Execute(..))
        {
            // If the proving key does not exist, then synthesize it.
            if !self.contains_proving_key(function.name()) {
                // Add the circuit key to the mapping.
                self.synthesize_from_assignment(function.name(), &assignment)?;
                lap!(timer, "Synthesize the {} circuit key", function.name());
            }
        }

        // If the circuit is in `CheckDeployment` mode, then save the assignment.
        if let CallStack::CheckDeployment(_, _, ref assignments) = registers.call_stack() {
            // Construct the call metrics.
            let metrics = CallMetrics {
                program_id: *self.program_id(),
                function_name: *function.name(),
                num_instructions: function.instructions().len(),
                num_request_constraints,
                num_function_constraints,
                num_response_constraints,
            };
            // Add the assignment to the assignments.
            assignments.write().push((assignment, metrics));
            lap!(timer, "Save the circuit assignment");
        }
        // If the circuit is in `Execute` mode, then execute the circuit into a transition.
        else if let CallStack::Execute(_, ref trace) = registers.call_stack() {
            registers.ensure_console_and_circuit_registers_match()?;

            // Construct the transition.
            let transition = Transition::from(&console_request, &response, finalize, &output_types, &output_registers)?;
            // Retrieve the proving key.
            let proving_key = self.get_proving_key(function.name())?;
            // Construct the call metrics.
            let metrics = CallMetrics {
                program_id: *self.program_id(),
                function_name: *function.name(),
                num_instructions: function.instructions().len(),
                num_request_constraints,
                num_function_constraints,
                num_response_constraints,
            };

            // Add the transition to the trace.
            trace.write().insert_transition(
                console_request.input_ids(),
                &transition,
                (proving_key, assignment),
                metrics,
            )?;
        }

        finish!(timer);

        // Return the response.
        Ok(response)
    }
}

impl<N: Network> Stack<N> {
    /// Prints the current state of the circuit.
    #[cfg(debug_assertions)]
    pub(crate) fn log_circuit<A: circuit::Aleo<Network = N>, S: Into<String>>(scope: S) {
        use colored::Colorize;

        // Determine if the circuit is satisfied.
        let is_satisfied = if A::is_satisfied() { "✅".green() } else { "❌".red() };
        // Determine the count.
        let (num_constant, num_public, num_private, num_constraints, num_nonzeros) = A::count();

        // Print the log.
        println!(
            "{is_satisfied} {:width$} (Constant: {num_constant}, Public: {num_public}, Private: {num_private}, Constraints: {num_constraints}, NonZeros: {num_nonzeros:?})",
            scope.into().bold(),
            width = 20
        );
    }
}

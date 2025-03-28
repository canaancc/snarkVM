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

impl<N: Network> ToBits for Entry<N, Plaintext<N>> {
    /// Returns this entry as a list of **little-endian** bits.
    fn to_bits_le(&self) -> Vec<bool> {
        let mut bits_le = match self {
            Self::Constant(..) => vec![false, false],
            Self::Public(..) => vec![false, true],
            Self::Private(..) => vec![true, false],
        };
        match self {
            Self::Constant(plaintext) => bits_le.extend(plaintext.to_bits_le()),
            Self::Public(plaintext) => bits_le.extend(plaintext.to_bits_le()),
            Self::Private(plaintext) => bits_le.extend(plaintext.to_bits_le()),
        }
        bits_le
    }

    /// Returns this entry as a list of **big-endian** bits.
    fn to_bits_be(&self) -> Vec<bool> {
        let mut bits_be = match self {
            Self::Constant(..) => vec![false, false],
            Self::Public(..) => vec![false, true],
            Self::Private(..) => vec![true, false],
        };
        match self {
            Self::Constant(plaintext) => bits_be.extend(plaintext.to_bits_be()),
            Self::Public(plaintext) => bits_be.extend(plaintext.to_bits_be()),
            Self::Private(plaintext) => bits_be.extend(plaintext.to_bits_be()),
        }
        bits_be
    }
}

impl<N: Network> ToBits for Entry<N, Ciphertext<N>> {
    /// Returns this entry as a list of **little-endian** bits.
    fn to_bits_le(&self) -> Vec<bool> {
        let mut bits_le = match self {
            Self::Constant(..) => vec![false, false],
            Self::Public(..) => vec![false, true],
            Self::Private(..) => vec![true, false],
        };
        match self {
            Self::Constant(plaintext) => bits_le.extend(plaintext.to_bits_le()),
            Self::Public(plaintext) => bits_le.extend(plaintext.to_bits_le()),
            Self::Private(plaintext) => bits_le.extend(plaintext.to_bits_le()),
        }
        bits_le
    }

    /// Returns this entry as a list of **big-endian** bits.
    fn to_bits_be(&self) -> Vec<bool> {
        let mut bits_be = match self {
            Self::Constant(..) => vec![false, false],
            Self::Public(..) => vec![false, true],
            Self::Private(..) => vec![true, false],
        };
        match self {
            Self::Constant(plaintext) => bits_be.extend(plaintext.to_bits_be()),
            Self::Public(plaintext) => bits_be.extend(plaintext.to_bits_be()),
            Self::Private(plaintext) => bits_be.extend(plaintext.to_bits_be()),
        }
        bits_be
    }
}

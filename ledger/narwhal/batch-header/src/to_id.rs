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

impl<N: Network> BatchHeader<N> {
    /// Returns the batch ID.
    pub fn to_id(&self) -> Result<Field<N>> {
        Self::compute_batch_id(
            self.version,
            self.author,
            self.round,
            self.timestamp,
            &self.transmission_ids,
            &self.previous_certificate_ids,
            &self.last_committed_certificate_ids,
        )
    }
}

impl<N: Network> BatchHeader<N> {
    /// Returns the batch ID.
    pub fn compute_batch_id(
        version: u8,
        author: Address<N>,
        round: u64,
        timestamp: i64,
        transmission_ids: &IndexSet<TransmissionID<N>>,
        previous_certificate_ids: &IndexSet<Field<N>>,
        last_committed_certificate_ids: &IndexSet<Field<N>>,
    ) -> Result<Field<N>> {
        let mut preimage = Vec::new();
        // Insert the author.
        author.write_le(&mut preimage)?;
        // Insert the round number.
        round.write_le(&mut preimage)?;
        // Insert the timestamp.
        timestamp.write_le(&mut preimage)?;
        // Insert the number of transmissions.
        u32::try_from(transmission_ids.len())?.write_le(&mut preimage)?;
        // Insert the transmission IDs.
        for transmission_id in transmission_ids {
            transmission_id.write_le(&mut preimage)?;
        }
        // Insert the number of previous certificate IDs.
        u32::try_from(previous_certificate_ids.len())?.write_le(&mut preimage)?;
        // Insert the previous certificate IDs.
        for certificate_id in previous_certificate_ids {
            // Insert the certificate ID.
            certificate_id.write_le(&mut preimage)?;
        }
        // TODO (howardwu): For mainnet - Change this to always encode the number of committed certificate IDs.
        //  We currently only encode the size and certificates only in the new version, for backwards compatibility.
        if version != 1 {
            // Insert the number of last committed certificate IDs.
            u32::try_from(last_committed_certificate_ids.len())?.write_le(&mut preimage)?;
            // Insert the last committed certificate IDs.
            for certificate_id in last_committed_certificate_ids {
                // Insert the certificate ID.
                certificate_id.write_le(&mut preimage)?;
            }
        }
        // Hash the preimage.
        N::hash_bhp1024(&preimage.to_bits_le())
    }
}

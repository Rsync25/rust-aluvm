// AluRE: AluVM runtime environment.
// This is rust implementation of AluVM (arithmetic logic unit virtual machine).
//
// Designed & written in 2021 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//
// This software is licensed under the terms of MIT License.
// You should have received a copy of the MIT License along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg_attr(feature = "std", macro_use)]
extern crate amplify;
#[macro_use]
extern crate bitcoin_hashes;

pub mod instr;
pub mod registers;
#[cfg(feature = "std")]
mod runtime;
mod types;

pub use instr::{Instr, InstructionSet};
#[cfg(feature = "std")]
pub use runtime::Runtime;
pub use types::Lib;
pub use types::{Blob, LibHash, LibSite, LiteralParseError, Value};

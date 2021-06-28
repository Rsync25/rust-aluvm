// Reference rust implementation of AluVM (arithmetic logic unit virtual machine).
// To find more on AluVM please check <https://github.com/internet2-org/aluvm-spec>
//
// Designed & written in 2021 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
// This work is donated to LNP/BP Standards Association by Pandora Core AG
//
// This software is licensed under the terms of MIT License.
// You should have received a copy of the MIT License along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use core::marker::PhantomData;
use core::ops::RangeInclusive;

use bitcoin_hashes::Hash;

use crate::instr::serialize::{compile, Bytecode, EncodeError};
use crate::instr::{ExecStep, NOp};
use crate::{Cursor, Instr, InstructionSet, Registers};

const LIB_HASH_MIDSTATE: [u8; 32] = [
    156, 224, 228, 230, 124, 17, 108, 57, 56, 179, 202, 242, 195, 15, 80, 137, 211, 243, 147, 108,
    71, 99, 110, 96, 125, 179, 62, 234, 221, 198, 240, 201,
];

sha256t_hash_newtype!(
    LibHash,
    LibHashTag,
    LIB_HASH_MIDSTATE,
    64,
    doc = "Library reference: a hash of the library code",
    false
);

/// AluVM executable code library
#[derive(Debug, Display)]
#[display("{bytecode}", alt = "{bytecode:#}")]
pub struct Lib<E = NOp>
where
    E: InstructionSet,
{
    bytecode: ByteStr,
    instruction_set: PhantomData<E>,
}

impl<E> Lib<E>
where
    E: InstructionSet,
{
    /// Constructs library for the provided instructions by encoding them into
    /// bytecode
    pub fn with<I>(code: I) -> Result<Lib<E>, EncodeError>
    where
        I: IntoIterator,
        <I as IntoIterator>::Item: InstructionSet,
    {
        let bytecode = compile::<E, _>(code)?;
        Ok(Lib { bytecode, instruction_set: PhantomData::<E>::default() })
    }

    /// Returns hash identifier [`LibHash`], representing the library in a
    /// unique way.
    ///
    /// Lib hash is computed as SHA256 tagged hash of the serialized library
    /// bytecode.
    pub fn lib_hash(&self) -> LibHash { LibHash::hash(&*self.bytecode.bytes) }

    /// Calculates length of bytecode encoding in bytes
    pub fn byte_count(&self) -> usize { self.bytecode.len() }

    /// Returns bytecode reference
    pub fn bytecode(&self) -> &[u8] { &self.bytecode.as_ref() }

    /// Executes library code starting at entrypoint
    pub fn run(&self, entrypoint: u16, registers: &mut Registers) -> Option<LibSite> {
        let mut cursor = Cursor::with(&self.bytecode.bytes[..]);
        let lib_hash = self.lib_hash();
        cursor.seek(entrypoint);

        while !cursor.is_eof() {
            let instr = Instr::<E>::read(&mut cursor).ok()?;
            match instr.exec(registers, LibSite::with(cursor.pos(), lib_hash)) {
                ExecStep::Stop => return None,
                ExecStep::Next => continue,
                ExecStep::Jump(pos) => cursor.seek(pos),
                ExecStep::Call(site) => return Some(site),
            }
        }

        None
    }
}

impl<E> AsRef<[u8]> for Lib<E>
where
    E: InstructionSet,
{
    fn as_ref(&self) -> &[u8] { self.bytecode.as_ref() }
}

/// Location within a library
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Display)]
#[display("{pos:#06X}@{lib}")]
pub struct LibSite {
    /// Library hash
    pub lib: LibHash,

    /// Offset from the beginning of the code, in bytes
    pub pos: u16,
}

impl LibSite {
    /// Constricts library site reference from a given position and library hash
    /// value
    pub fn with(pos: u16, lib: LibHash) -> LibSite { LibSite { lib, pos } }
}

/// Large binary bytestring object.
///
/// NB: Since byte string length is expressed with `u16` integer, it is 0-based, i.e. one character
/// string has length of `0`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ByteStr {
    /// Adjusted slice length.
    ///
    /// Values from `0` to `u16:MAX` represent string length (with `0` meaning "no value is
    /// stored").
    ///
    /// `None` value indicates that the data occupy full length (i.e. `u16::MAX + 1`).
    len: Option<u16>,

    /// Slice bytes
    pub bytes: Box<[u8; u16::MAX as usize]>,
}

impl Default for ByteStr {
    fn default() -> ByteStr { ByteStr { len: Some(0), bytes: Box::new([0u8; u16::MAX as usize]) } }
}

impl AsRef<[u8]> for ByteStr {
    fn as_ref(&self) -> &[u8] { &self.bytes[..self.len()] }
}

impl ByteStr {
    /// Constructs blob from slice of bytes.
    ///
    /// Panics if the length of the slice is greater than `u16::MAX` bytes.
    pub fn with(slice: impl AsRef<[u8]>) -> ByteStr {
        let len = slice.as_ref().len();
        assert!(len <= u16::MAX as usize + 1);
        let mut bytes = [0u8; u16::MAX as usize];
        bytes[0..len].copy_from_slice(slice.as_ref());
        ByteStr {
            len: if len > u16::MAX as usize { None } else { Some(len as u16) },
            bytes: Box::new(bytes),
        }
    }

    /// Returns correct length of the string, in range `0 ..= u16::MAX + 1`
    #[inline]
    pub fn len(&self) -> usize { self.len.map(|len| len as usize).unwrap_or(u16::MAX as usize + 1) }

    /// Adjusts the length of the string if necessary
    #[inline]
    pub fn adjust_len(&mut self, new_len: u16, inclusive: bool) {
        match (self.len, new_len, inclusive) {
            (Some(_), u16::MAX, true) => self.len = None,
            (Some(len), new, true) if len <= new => self.len = Some(new + 1),
            (Some(len), new, false) if len < new => self.len = Some(new),
            _ => {}
        }
    }

    /// Fills range within a string with the provided byte value, increasing string length if
    /// necessary
    pub fn fill(&mut self, range: RangeInclusive<u16>, val: u8) {
        let start = *range.start();
        let end = *range.end();
        self.adjust_len(end, true);
        self.bytes[start as usize..=end as usize].fill(val);
    }
}

#[cfg(feature = "std")]
impl Display for ByteStr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use amplify_num::hex::ToHex;
        let vec = Vec::from(&self.bytes[..self.len()]);
        if let Ok(s) = String::from_utf8(vec) {
            f.write_str("\"")?;
            f.write_str(&s)?;
            f.write_str("\"")
        } else if f.alternate() && self.len() > 4 {
            write!(f, "{}..{}", self.bytes[..4].to_hex(), self.bytes[(self.len() - 4)..].to_hex())
        } else {
            f.write_str(&self.bytes[0usize..(self.len())].to_hex())
        }
    }
}

#[cfg(not(feature = "std"))]
impl Display for ByteStr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let vec = Vec::from(&self.bytes[..self.len()]);
        write!(f, "{:#04X?}", &self.bytes[0usize..(self.len())])
    }
}

/// Error returned by [`Runtime::call`] method when the code calls to a library
/// not known to the runtime
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display)]
#[display("call to unknown library {0:#}")]
#[cfg_attr(feature = "std", derive(Error))]
pub struct NoLibraryError(LibHash);

/// AluVM runtime execution environment
#[derive(Getters, Debug, Default)]
pub struct Runtime<E = NOp>
where
    E: InstructionSet,
{
    /// Libraries known to the runtime, identified by their hashes
    libs: BTreeMap<LibHash, Lib<E>>,

    /// Entrypoint for the main function
    entrypoint: LibSite,

    /// A set of registers
    registers: Registers,
}

impl<E> Runtime<E>
where
    E: InstructionSet,
{
    pub fn new() -> Runtime<E> {
        Runtime {
            libs: Default::default(),
            entrypoint: Default::default(),
            registers: Default::default(),
        }
    }

    pub fn with(lib: Lib<E>) -> Runtime<E> {
        let mut runtime = Runtime::new();
        runtime.entrypoint = LibSite::with(0, lib.lib_hash());
        runtime.add_lib(lib);
        runtime
    }

    /// Adds Alu bytecode library to the runtime environment. Returns if the
    /// library was already known.
    pub fn add_lib(&mut self, lib: Lib<E>) -> bool {
        self.libs.insert(lib.lib_hash(), lib).is_none()
    }

    pub fn set_entrypoint(&mut self, entrypoint: LibSite) { self.entrypoint = entrypoint; }

    pub fn main(&mut self) -> Result<bool, NoLibraryError> { self.call(self.entrypoint) }

    pub fn call(&mut self, mut method: LibSite) -> Result<bool, NoLibraryError> {
        while let Some(m) = self
            .libs
            .get(&method.lib)
            .ok_or(NoLibraryError(method.lib))?
            .run(method.pos, &mut self.registers)
        {
            method = m
        }
        Ok(self.registers.st0)
    }
}

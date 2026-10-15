//! A buffered, seekable reader for parsing binary headers.
//!
//! Parsers skip over pixel data with `seek_relative`, which stays inside the
//! read buffer when it can, so inspecting a file reads its headers rather
//! than its whole contents.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

pub struct Source<R> {
    inner: BufReader<R>,
    len: u64,
}

impl<R: Read + Seek> Source<R> {
    pub fn new(mut reader: R) -> io::Result<Self> {
        let len = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner: BufReader::with_capacity(16 * 1024, reader),
            len,
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn position(&mut self) -> io::Result<u64> {
        self.inner.stream_position()
    }

    pub fn remaining(&mut self) -> io::Result<u64> {
        Ok(self.len.saturating_sub(self.position()?))
    }

    /// The bytes at the start of the file, without consuming them. Returns up
    /// to the buffer size; fewer only for short files.
    pub fn peek_start(&mut self) -> io::Result<&[u8]> {
        self.inner.seek(SeekFrom::Start(0))?;
        self.inner.fill_buf()
    }

    pub fn rewind(&mut self) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(0)).map(|_| ())
    }

    pub fn seek_to(&mut self, offset: u64) -> io::Result<()> {
        if offset > self.len {
            return Err(eof());
        }
        let delta = i64::try_from(i128::from(offset) - i128::from(self.position()?))
            .map_err(|_| invalid("offset out of range"))?;
        self.inner.seek_relative(delta)
    }

    /// Skips `n` bytes forward. Errors if that would pass the end of the file,
    /// so a corrupt length can't send a parser into an endless loop.
    pub fn skip(&mut self, n: u64) -> io::Result<()> {
        let target = self.position()?.checked_add(n).ok_or_else(eof)?;
        self.seek_to(target)
    }

    pub fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        if len as u64 > self.remaining()? {
            return Err(eof());
        }
        let mut buf = vec![0u8; len];
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn u8(&mut self) -> io::Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16_be(&mut self) -> io::Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn u16_le(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32_be(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn u32_le(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn u64_be(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
}

pub fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "file ends inside a header")
}

pub fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
pub fn source(bytes: Vec<u8>) -> Source<io::Cursor<Vec<u8>>> {
    Source::new(io::Cursor::new(bytes)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_values_and_tracks_position() {
        let mut s = source(vec![0x12, 0x34, 0x56, 0x78, 0xAA]);
        assert_eq!(s.u16_be().unwrap(), 0x1234);
        assert_eq!(s.u16_le().unwrap(), 0x7856);
        assert_eq!(s.position().unwrap(), 4);
        assert_eq!(s.remaining().unwrap(), 1);
    }

    #[test]
    fn skipping_past_the_end_is_an_error() {
        let mut s = source(vec![0; 10]);
        s.skip(10).unwrap();
        assert!(s.skip(1).is_err());
        assert!(s.bytes(1).is_err());
    }

    #[test]
    fn peek_does_not_consume() {
        let mut s = source(b"GIF89a".to_vec());
        assert_eq!(&s.peek_start().unwrap()[..3], b"GIF");
        assert_eq!(s.array::<6>().unwrap(), *b"GIF89a");
    }
}

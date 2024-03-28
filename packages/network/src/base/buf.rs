use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub enum ReadOnlyBuffer<'a> {
    Ref(&'a [u8]),
    Vec(Vec<u8>),
}

impl<'a> ReadOnlyBuffer<'a> {
    pub fn view<'b>(&'b self) -> ReadOnlyBuffer<'b> {
        match self {
            Self::Ref(r) => ReadOnlyBuffer::Ref(r),
            Self::Vec(v) => ReadOnlyBuffer::Ref(v),
        }
    }

    pub fn owned(self) -> ReadOnlyBuffer<'static> {
        match self {
            Self::Ref(r) => ReadOnlyBuffer::Vec(r.to_vec()),
            Self::Vec(v) => ReadOnlyBuffer::Vec(v),
        }
    }

    pub fn owned_mut(self) -> WriteableBuffer<'static> {
        match self {
            Self::Ref(r) => WriteableBuffer::Vec(r.to_vec()),
            Self::Vec(v) => WriteableBuffer::Vec(v),
        }
    }

    pub fn clone_mut(&self) -> WriteableBuffer<'static> {
        match self {
            Self::Ref(r) => WriteableBuffer::Vec(r.to_vec()),
            Self::Vec(v) => WriteableBuffer::Vec(v.to_vec()),
        }
    }
}

impl<'a> Deref for ReadOnlyBuffer<'a> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(r) => r,
            Self::Vec(v) => v,
        }
    }
}

impl<'a> From<&'a [u8]> for ReadOnlyBuffer<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Ref(value)
    }
}

impl<'a> From<Vec<u8>> for ReadOnlyBuffer<'a> {
    fn from(value: Vec<u8>) -> Self {
        Self::Vec(value)
    }
}

#[derive(Debug)]
pub enum WriteableBuffer<'a> {
    Ref(&'a mut [u8]),
    Vec(Vec<u8>),
}

impl<'a> WriteableBuffer<'a> {
    pub fn owned(self) -> WriteableBuffer<'static> {
        match self {
            Self::Ref(r) => WriteableBuffer::Vec(r.to_vec()),
            Self::Vec(v) => WriteableBuffer::Vec(v),
        }
    }

    pub fn freeze(self) -> ReadOnlyBuffer<'a> {
        match self {
            Self::Ref(r) => ReadOnlyBuffer::Ref(r),
            Self::Vec(v) => ReadOnlyBuffer::Vec(v),
        }
    }

    pub fn copy_readonly(&self) -> ReadOnlyBuffer<'static> {
        match self {
            Self::Ref(r) => ReadOnlyBuffer::Vec(r.to_vec()),
            Self::Vec(v) => ReadOnlyBuffer::Vec(v.clone()),
        }
    }

    pub fn view<'b>(&'b self) -> ReadOnlyBuffer<'b> {
        match self {
            Self::Ref(r) => ReadOnlyBuffer::Ref(r),
            Self::Vec(v) => ReadOnlyBuffer::Ref(v),
        }
    }

    pub fn ensure_back(&mut self, more: usize) {
        match self {
            Self::Ref(r) => {
                let mut v = Vec::with_capacity(r.len() + more);
                v[0..r.len()].copy_from_slice(r);
                unsafe {
                    v.set_len(r.len() + more);
                }
            }
            Self::Vec(v) => {
                v.reserve_exact(more);
                unsafe {
                    v.set_len(v.capacity());
                }
            }
        }
    }
}

impl<'a> Deref for WriteableBuffer<'a> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(r) => r,
            Self::Vec(v) => v,
        }
    }
}

impl<'a> DerefMut for WriteableBuffer<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Ref(r) => r,
            Self::Vec(v) => v,
        }
    }
}

impl<'a> From<&'a mut [u8]> for WriteableBuffer<'a> {
    fn from(value: &'a mut [u8]) -> Self {
        Self::Ref(value)
    }
}

impl<'a> From<Vec<u8>> for WriteableBuffer<'a> {
    fn from(value: Vec<u8>) -> Self {
        Self::Vec(value)
    }
}

#[derive(Debug, Clone)]
pub struct GenericBuffer<'a> {
    pub buf: ReadOnlyBuffer<'a>,
    pub range: std::ops::Range<usize>,
}

impl<'a> GenericBuffer<'a> {
    pub fn owned(self) -> GenericBuffer<'static> {
        GenericBuffer {
            buf: self.buf.owned(),
            range: self.range,
        }
    }

    pub fn owned_mut(self) -> GenericBufferMut<'static> {
        GenericBufferMut {
            buf: self.buf.owned_mut(),
            range: self.range,
        }
    }

    pub fn clone_mut(&self) -> GenericBufferMut<'static> {
        GenericBufferMut {
            buf: self.buf.clone_mut(),
            range: self.range.clone(),
        }
    }

    pub fn view<'b>(&'b self, range: std::ops::Range<usize>) -> Option<GenericBuffer<'b>> {
        if self.range.end - self.range.start >= range.end {
            Some(GenericBuffer {
                buf: self.buf.view(),
                range: (self.range.start + range.start..self.range.start + range.end),
            })
        } else {
            None
        }
    }

    pub fn pop_back<'b>(&'b mut self, len: usize) -> Option<GenericBuffer<'b>> {
        if self.range.end - self.range.start >= len {
            self.range.end -= len;
            Some(GenericBuffer {
                buf: self.buf.view(),
                range: (self.range.end..self.range.end + len),
            })
        } else {
            None
        }
    }

    pub fn pop_front<'b>(&'b mut self, len: usize) -> Option<GenericBuffer<'b>> {
        if self.range.end - self.range.start >= len {
            self.range.start += len;
            Some(GenericBuffer {
                buf: self.buf.view(),
                range: (self.range.start - len..self.range.start),
            })
        } else {
            None
        }
    }

    pub fn to_slice2(&self) -> &'_ [u8] {
        &self.buf.deref()[self.range.clone()]
    }
}

impl<'a> Deref for GenericBuffer<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buf.deref()[self.range.clone()]
    }
}

impl<'a> From<&'a [u8]> for GenericBuffer<'a> {
    fn from(value: &'a [u8]) -> Self {
        GenericBuffer {
            buf: value.into(),
            range: (0..value.len()),
        }
    }
}

impl From<Vec<u8>> for GenericBuffer<'_> {
    fn from(value: Vec<u8>) -> Self {
        let len = value.len();
        GenericBuffer { buf: value.into(), range: (0..len) }
    }
}

#[derive(Debug)]
pub struct GenericBufferMut<'a> {
    buf: WriteableBuffer<'a>,
    range: std::ops::Range<usize>,
}

impl<'a> GenericBufferMut<'a> {
    /// Create a buffer mut with append some bytes at first and some bytes at end
    pub fn build(data: &[u8], more_left: usize, more_right: usize) -> GenericBufferMut<'static> {
        let mut v = Vec::with_capacity(more_left + data.len() + more_right);
        unsafe {
            v.set_len(v.capacity());
        };
        v[more_left..more_left + data.len()].copy_from_slice(data);
        GenericBufferMut {
            buf: v.into(),
            range: (more_left..more_left + data.len()),
        }
    }

    /// Create a new buffer from a slice, we can manually set the start and end of the buffer.
    pub fn from_slice_raw(data: &'a mut [u8], range: std::ops::Range<usize>) -> GenericBufferMut<'a> {
        assert!(range.end <= data.len());
        GenericBufferMut { buf: data.into(), range }
    }

    /// Create a new buffer from a vec, we can manually set the start and end of the buffer.
    pub fn from_vec_raw(data: Vec<u8>, range: std::ops::Range<usize>) -> GenericBufferMut<'a> {
        assert!(range.end <= data.len());
        GenericBufferMut { buf: data.into(), range }
    }

    pub fn freeze(self) -> GenericBuffer<'a> {
        GenericBuffer {
            buf: self.buf.freeze(),
            range: self.range,
        }
    }

    pub fn copy_readonly(&self) -> GenericBuffer<'static> {
        GenericBuffer {
            buf: self.buf.copy_readonly(),
            range: self.range.clone(),
        }
    }

    /// Reverse the buffer for at least `len` bytes at back.
    pub fn ensure_back(&mut self, more: usize) {
        assert!(self.buf.len() >= self.range.end);
        let remain = self.buf.len() - self.range.end;
        if remain < more {
            self.buf.ensure_back(more - remain);
        }
    }

    pub fn truncate(&mut self, len: usize) -> Option<()> {
        if self.range.end - self.range.start >= len {
            self.range.end = self.range.start + len;
            Some(())
        } else {
            None
        }
    }

    pub fn push_back(&mut self, data: &[u8]) {
        self.ensure_back(data.len());
        self.buf.deref_mut()[self.range.end..self.range.end + data.len()].copy_from_slice(data);
        self.range.end += data.len();
    }

    pub fn pop_back(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        if self.range.end - self.range.start >= len {
            self.range.end -= len;
            Some(GenericBuffer {
                buf: self.buf.view(),
                range: (self.range.end..self.range.end + len),
            })
        } else {
            None
        }
    }

    pub fn pop_front(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        if self.range.end - self.range.start >= len {
            self.range.start += len;
            Some(GenericBuffer {
                buf: self.buf.view(),
                range: (self.range.start - len..self.range.start),
            })
        } else {
            None
        }
    }

    pub fn move_front_right(&mut self, len: usize) -> Option<()> {
        if self.range.start + len > self.range.end {
            return None;
        }
        self.range.start += len;
        Some(())
    }

    pub fn move_front_left(&mut self, len: usize) -> Option<()> {
        if self.range.start < len {
            return None;
        }
        self.range.start -= len;
        Some(())
    }
}

impl<'a> From<&'a mut [u8]> for GenericBufferMut<'a> {
    fn from(value: &'a mut [u8]) -> Self {
        GenericBufferMut {
            range: (0..value.len()),
            buf: value.into(),
        }
    }
}

impl From<Vec<u8>> for GenericBufferMut<'_> {
    fn from(value: Vec<u8>) -> Self {
        GenericBufferMut {
            range: (0..value.len()),
            buf: value.into(),
        }
    }
}

impl<'a> Deref for GenericBufferMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buf.deref()[self.range.start..self.range.end]
    }
}

impl<'a> DerefMut for GenericBufferMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf.deref_mut()[self.range.start..self.range.end]
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::{GenericBuffer, GenericBufferMut};

    #[test]
    fn simple_buffer_view() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let mut buf: GenericBuffer = (data.as_slice()).into();
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.pop_back(2).expect("").deref(), &[5, 6]);
        assert_eq!(buf.pop_front(2).expect("").deref(), &[1, 2]);
        assert_eq!(buf.deref(), &[3, 4]);
    }

    #[test]
    fn simple_buffer_mut() {
        let mut buf = GenericBufferMut::build(&[1, 2, 3, 4, 5, 6], 4, 4);
        assert_eq!(buf.deref(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(buf.to_vec(), vec![1, 2, 3, 4, 5, 6]);
        println!("{:?}", buf);
        let res = buf.pop_back(2).expect("");
        println!("{:?}", res);
        assert_eq!(res.deref(), &[5, 6]);
        assert_eq!(res.to_vec(), &[5, 6]);
        assert_eq!(buf.pop_front(2).expect("").deref(), &[1, 2]);
    }
}

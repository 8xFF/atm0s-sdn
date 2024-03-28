use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub enum GenericBuffer<'a> {
    Ref(&'a [u8], usize, usize),
    Vec(Vec<u8>, usize, usize),
}

impl<'a> GenericBuffer<'a> {
    pub fn owned(self) -> GenericBuffer<'static> {
        match self {
            GenericBuffer::Ref(r, start, end) => GenericBuffer::Vec(r.to_vec(), start, end),
            GenericBuffer::Vec(v, start, end) => GenericBuffer::Vec(v, start, end),
        }
    }

    pub fn owned_mut(self) -> GenericBufferMut<'static> {
        match self {
            GenericBuffer::Ref(r, start, end) => GenericBufferMut::Vec(r.to_vec(), start, end),
            GenericBuffer::Vec(v, start, end) => GenericBufferMut::Vec(v, start, end),
        }
    }

    pub fn clone_mut(&self) -> GenericBufferMut<'static> {
        match self {
            GenericBuffer::Ref(r, start, end) => GenericBufferMut::Vec(r.to_vec(), *start, *end),
            GenericBuffer::Vec(v, start, end) => GenericBufferMut::Vec(v.to_vec(), *start, *end),
        }
    }

    pub fn sub_view(&self, range: std::ops::Range<usize>) -> GenericBuffer<'_> {
        match self {
            GenericBuffer::Ref(r, start, _end) => GenericBuffer::Ref(r, start + range.start, start + range.end),
            GenericBuffer::Vec(v, start, _end) => GenericBuffer::Ref(v, start + range.start, start + range.end),
        }
    }

    pub fn pop_back(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        match self {
            GenericBuffer::Ref(r, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *end -= len;
                Some(GenericBuffer::Ref(r, *end, *end + len))
            }
            GenericBuffer::Vec(v, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *end -= len;
                Some(GenericBuffer::Ref(v, *end, *end + len))
            }
        }
    }

    pub fn pop_front(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        match self {
            GenericBuffer::Ref(r, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *start += len;
                Some(GenericBuffer::Ref(r, *start - len, *start))
            }
            GenericBuffer::Vec(v, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *start += len;
                Some(GenericBuffer::Ref(v, *start - len, *start))
            }
        }
    }
}

impl<'a> Deref for GenericBuffer<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            GenericBuffer::Ref(r, start, end) => &r[*start..*end],
            GenericBuffer::Vec(v, start, end) => &v[*start..*end],
        }
    }
}

impl<'a> From<&'a [u8]> for GenericBuffer<'a> {
    fn from(value: &'a [u8]) -> Self {
        GenericBuffer::Ref(value, 0, value.len())
    }
}

impl From<Vec<u8>> for GenericBuffer<'_> {
    fn from(value: Vec<u8>) -> Self {
        let len = value.len();
        GenericBuffer::Vec(value, 0, len)
    }
}

#[derive(Debug)]
pub enum GenericBufferMut<'a> {
    Ref(&'a mut [u8], usize, usize),
    Vec(Vec<u8>, usize, usize),
}

impl<'a> GenericBufferMut<'a> {
    /// Create a new buffer from a slice with append more some padding before and after the slice.
    pub fn create_from_slice(data: &[u8], before: usize, after: usize) -> GenericBufferMut<'a> {
        let mut buf = Vec::with_capacity(before + data.len() + after);
        //TODO remove unsafe
        unsafe {
            buf.set_len(before + data.len() + after);
        }
        buf[before..(before + data.len())].copy_from_slice(data);
        GenericBufferMut::Vec(buf, before, before + data.len())
    }

    /// Create a new buffer from a slice, we can manually set the start and end of the buffer.
    pub fn from_slice_raw(data: Vec<u8>, before: usize, after: usize) -> GenericBufferMut<'a> {
        assert!(after <= data.len());
        assert!(before <= after);
        GenericBufferMut::Vec(data, before, after)
    }

    /// Create a new buffer from a vec, we can manually set the start and end of the buffer.
    pub fn from_vec_raw(data: Vec<u8>, before: usize, after: usize) -> GenericBufferMut<'a> {
        assert!(after <= data.len());
        assert!(before <= after);
        GenericBufferMut::Vec(data, before, after)
    }

    pub fn to_readonly(self) -> GenericBuffer<'a> {
        match self {
            GenericBufferMut::Ref(r, start, end) => GenericBuffer::Ref(r, start, end),
            GenericBufferMut::Vec(v, start, end) => GenericBuffer::Vec(v, start, end),
        }
    }

    pub fn copy_readonly(&self) -> GenericBuffer<'static> {
        match self {
            GenericBufferMut::Ref(r, start, end) => GenericBuffer::Vec(r.to_vec(), *start, *end),
            GenericBufferMut::Vec(v, start, end) => GenericBuffer::Vec(v.clone(), *start, *end),
        }
    }

    /// Reverse the buffer for at least `len` bytes at back.
    pub fn ensure_back(&mut self, len: usize) {
        match self {
            GenericBufferMut::Ref(r, start, end) => {
                if len > r.len() - *end {
                    let mut new = Vec::with_capacity(r.len() + len);
                    new.extend_from_slice(&r[*start..*end]);
                    *self = GenericBufferMut::Vec(new, 0, *end - *start);
                }
            }
            GenericBufferMut::Vec(v, _start, end) => {
                if len > v.len() - *end {
                    log::debug!("Ensure back {} {}, current cap {}", len, v.len() - *end, v.len());
                    v.reserve(len - (v.len() - *end));
                    unsafe {
                        v.set_len(*end + len);
                    }
                    log::debug!("Ensure back after cap {}", v.len());
                }
            }
        }
    }

    pub fn truncate(&mut self, len: usize) -> Option<()> {
        match self {
            GenericBufferMut::Ref(_, start, end) | GenericBufferMut::Vec(_, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *end = *start + len;
                Some(())
            }
        }
    }

    pub fn extend(&mut self, data: &[u8]) -> Option<()> {
        match self {
            GenericBufferMut::Ref(r, _start, end) => {
                if data.len() > r.len() - *end {
                    return None;
                }
                r[*end..*end + data.len()].copy_from_slice(data);
                *end += data.len();
                Some(())
            }
            GenericBufferMut::Vec(v, _start, end) => {
                if data.len() > v.len() - *end {
                    return None;
                }
                v[*end..*end + data.len()].copy_from_slice(data);
                *end += data.len();
                Some(())
            }
        }
    }

    pub fn pop_back(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        match self {
            GenericBufferMut::Ref(r, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *end -= len;
                Some(GenericBuffer::Ref(r, *end, *end + len))
            }
            GenericBufferMut::Vec(v, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *end -= len;
                Some(GenericBuffer::Ref(v, *end, *end + len))
            }
        }
    }

    pub fn pop_front(&mut self, len: usize) -> Option<GenericBuffer<'_>> {
        match self {
            GenericBufferMut::Ref(r, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *start += len;
                Some(GenericBuffer::Ref(r, *start - len, *start))
            }
            GenericBufferMut::Vec(v, start, end) => {
                if len > *end - *start {
                    return None;
                }
                *start += len;
                Some(GenericBuffer::Ref(v, *start - len, *start))
            }
        }
    }

    pub fn move_front_right(&mut self, len: usize) -> Option<()> {
        match self {
            GenericBufferMut::Ref(_, start, end) | GenericBufferMut::Vec(_, start, end) => {
                if *start + len > *end {
                    return None;
                }
                *start += len;
                Some(())
            }
        }
    }

    pub fn move_front_left(&mut self, len: usize) -> Option<()> {
        match self {
            GenericBufferMut::Ref(_, start, _end) | GenericBufferMut::Vec(_, start, _end) => {
                if *start < len {
                    return None;
                }
                *start -= len;
                Some(())
            }
        }
    }
}

impl<'a> From<&'a mut [u8]> for GenericBufferMut<'a> {
    fn from(value: &'a mut [u8]) -> Self {
        GenericBufferMut::Ref(value, 0, value.len())
    }
}

impl From<Vec<u8>> for GenericBufferMut<'_> {
    fn from(value: Vec<u8>) -> Self {
        let len = value.len();
        GenericBufferMut::Vec(value, 0, len)
    }
}

impl<'a> Deref for GenericBufferMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            GenericBufferMut::Ref(r, start, end) => &r[*start..*end],
            GenericBufferMut::Vec(v, start, end) => &v[*start..*end],
        }
    }
}

impl<'a> DerefMut for GenericBufferMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            GenericBufferMut::Ref(r, start, end) => &mut r[*start..*end],
            GenericBufferMut::Vec(v, start, end) => &mut v[*start..*end],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::GenericBufferMut;

    #[test]
    fn simple_buffer_view() {}

    #[test]
    fn simple_buffer_mut() {
        let mut buf = GenericBufferMut::create_from_slice(&[1, 2, 3, 4, 5, 6], 4, 4);
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

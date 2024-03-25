use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub enum GenericBuffer<'a> {
    Ref(&'a [u8]),
    Vec(Vec<u8>),
}

impl<'a> GenericBuffer<'a> {
    pub fn owned(self) -> GenericBuffer<'static> {
        match self {
            GenericBuffer::Ref(r) => r.to_vec().into(),
            GenericBuffer::Vec(v) => v.into(),
        }
    }

    pub fn owned_mut(self) -> GenericBufferMut<'static> {
        match self {
            GenericBuffer::Ref(r) => r.to_vec().into(),
            GenericBuffer::Vec(v) => v.into(),
        }
    }

    pub fn clone_mut(&self) -> GenericBufferMut<'static> {
        match self {
            GenericBuffer::Ref(r) => r.to_vec().into(),
            GenericBuffer::Vec(v) => v.clone().into(),
        }
    }

    pub fn sub_view(&self, range: std::ops::Range<usize>) -> GenericBuffer<'a> {
        match self {
            GenericBuffer::Ref(r) => GenericBuffer::Ref(&r[range]),
            GenericBuffer::Vec(v) => GenericBuffer::Vec(v[range].to_vec()),
        }
    }
}

impl<'a> Deref for GenericBuffer<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            GenericBuffer::Ref(r) => r,
            GenericBuffer::Vec(v) => v,
        }
    }
}

impl<'a> From<&'a [u8]> for GenericBuffer<'a> {
    fn from(value: &'a [u8]) -> Self {
        GenericBuffer::Ref(value)
    }
}

impl From<Vec<u8>> for GenericBuffer<'_> {
    fn from(value: Vec<u8>) -> Self {
        GenericBuffer::Vec(value)
    }
}

#[derive(Debug)]
pub enum GenericBufferMut<'a> {
    Ref(&'a mut [u8]),
    Vec(Vec<u8>),
}

impl<'a> GenericBufferMut<'a> {
    pub fn to_readonly(self) -> GenericBuffer<'a> {
        match self {
            GenericBufferMut::Ref(r) => GenericBuffer::Ref(r),
            GenericBufferMut::Vec(v) => GenericBuffer::Vec(v),
        }
    }
}

impl<'a> From<&'a mut [u8]> for GenericBufferMut<'a> {
    fn from(value: &'a mut [u8]) -> Self {
        GenericBufferMut::Ref(value)
    }
}

impl From<Vec<u8>> for GenericBufferMut<'_> {
    fn from(value: Vec<u8>) -> Self {
        GenericBufferMut::Vec(value)
    }
}

impl<'a> Deref for GenericBufferMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            GenericBufferMut::Ref(r) => r,
            GenericBufferMut::Vec(v) => v,
        }
    }
}

impl<'a> DerefMut for GenericBufferMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            GenericBufferMut::Ref(r) => r,
            GenericBufferMut::Vec(v) => v,
        }
    }
}

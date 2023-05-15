// Copyright 2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::hash::{Hash, Hasher};
use std::borrow::Borrow;
use bluesea_identity::PeerId;

/// A `Key` in the DHT keyspace with preserved preimage.
///
/// Keys in the DHT keyspace identify both the participating nodes, as well as
/// the records stored in the DHT.
///
/// `Key`s have an XOR metric as defined in the Kademlia paper, i.e. the bitwise XOR of
/// the hash digests, interpreted as an integer. See [`Key::distance`].
#[derive(Clone, Debug)]
pub struct Key<T> {
    preimage: T,
    bytes: KeyBytes,
}

impl<T> Key<T> {
    /// Constructs a new `Key` by running the given value through a random
    /// oracle.
    ///
    /// The preimage of type `T` is preserved. See [`Key::preimage`] and
    /// [`Key::into_preimage`].
    pub fn new(preimage: T) -> Key<T>
        where
            T: Borrow<u32>,
    {
        let bytes = KeyBytes::new(preimage.borrow());
        Key { preimage, bytes }
    }

    /// Borrows the preimage of the key.
    pub fn preimage(&self) -> &T {
        &self.preimage
    }

    /// Converts the key into its preimage.
    pub fn into_preimage(self) -> T {
        self.preimage
    }

    /// Computes the distance of the keys according to the XOR metric.
    pub fn distance<U>(&self, other: &U) -> Distance
        where
            U: AsRef<KeyBytes>,
    {
        self.bytes.distance(other)
    }

    /// Returns the uniquely determined key with the given distance to `self`.
    ///
    /// This implements the following equivalence:
    ///
    /// `self xor other = distance <==> other = self xor distance`
    pub fn for_distance(&self, d: Distance) -> KeyBytes {
        self.bytes.for_distance(d)
    }
}

impl<T> From<Key<T>> for KeyBytes {
    fn from(key: Key<T>) -> KeyBytes {
        key.bytes
    }
}

impl From<PeerId> for Key<PeerId> {
    fn from(p: PeerId) -> Self {
        let bytes = KeyBytes(p);
        Key { preimage: p, bytes }
    }
}

impl<T> AsRef<KeyBytes> for Key<T> {
    fn as_ref(&self) -> &KeyBytes {
        &self.bytes
    }
}

impl<T, U> PartialEq<Key<U>> for Key<T> {
    fn eq(&self, other: &Key<U>) -> bool {
        self.bytes == other.bytes
    }
}

impl<T> Eq for Key<T> {}

/// The raw bytes of a key in the DHT keyspace.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct KeyBytes(u32);

impl KeyBytes {
    /// Creates a new key in the DHT keyspace by running the given
    /// value through a random oracle.
    pub fn new<T>(value: T) -> Self
        where
            T: Borrow<u32>,
    {
        KeyBytes(*value.borrow())
    }

    /// Computes the distance of the keys according to the XOR metric.
    pub fn distance<U>(&self, other: &U) -> Distance
        where
            U: AsRef<KeyBytes>,
    {
        let a = self.0;
        let b = other.as_ref().0;
        Distance(a ^ b)
    }

    /// Returns the uniquely determined key with the given distance to `self`.
    ///
    /// This implements the following equivalence:
    ///
    /// `self xor other = distance <==> other = self xor distance`
    pub fn for_distance(&self, d: Distance) -> KeyBytes {
        let key_int = self.0 ^ d.0;
        KeyBytes(key_int)
    }
}

impl AsRef<KeyBytes> for KeyBytes {
    fn as_ref(&self) -> &KeyBytes {
        self
    }
}

/// A distance between two keys in the DHT keyspace.
#[derive(Copy, Clone, PartialEq, Eq, Default, PartialOrd, Ord, Debug)]
pub struct Distance(pub(super) u32);

impl Distance {
    /// Returns the integer part of the base 2 logarithm of the [`Distance`].
    ///
    /// Returns `None` if the distance is zero.
    pub fn ilog2(&self) -> Option<u32> {
        (32 - self.0.leading_zeros()).checked_sub(1)
    }

    pub fn bit(&self, i: usize) -> bool {
        (self.0 & ((1 as u32) << i)) > 0
    }
}

#[cfg(test)]
mod tests {
    use bluesea_identity::PeerIdType;
    use super::*;
    use quickcheck::*;

    impl Arbitrary for Key<PeerId> {
        fn arbitrary(_: &mut Gen) -> Key<PeerId> {
            Key::from(PeerId::random())
        }
    }

    #[test]
    fn distance_bit() {
        let dis = Distance(0);
        assert_eq!(dis.bit(0), false);

        let dis = Distance(0b1);
        assert_eq!(dis.bit(0), true);
        assert_eq!(dis.bit(1), false);

        let dis = Distance(0b111);
        assert_eq!(dis.bit(0), true);
        assert_eq!(dis.bit(1), true);
        assert_eq!(dis.bit(2), true);
        assert_eq!(dis.bit(3), false);
    }

    #[test]
    fn identity() {
        fn prop(a: Key<PeerId>) -> bool {
            a.distance(&a) == Distance::default()
        }
        quickcheck(prop as fn(_) -> _)
    }

    #[test]
    fn symmetry() {
        fn prop(a: Key<PeerId>, b: Key<PeerId>) -> bool {
            a.distance(&b) == b.distance(&a)
        }
        quickcheck(prop as fn(_, _) -> _)
    }

    #[test]
    fn triangle_inequality() {
        fn prop(a: Key<PeerId>, b: Key<PeerId>, c: Key<PeerId>) -> TestResult {
            let ab = a.distance(&b);
            let bc = b.distance(&c);
            let (ab_plus_bc, overflow) = ab.0.overflowing_add(bc.0);
            if overflow {
                TestResult::discard()
            } else {
                TestResult::from_bool(a.distance(&c) <= Distance(ab_plus_bc))
            }
        }
        quickcheck(prop as fn(_, _, _) -> _)
    }

    #[test]
    fn unidirectionality() {
        fn prop(a: Key<PeerId>, b: Key<PeerId>) -> bool {
            let d = a.distance(&b);
            (0..100).all(|_| {
                let c = Key::from(PeerId::random());
                a.distance(&c) != d || b == c
            })
        }
        quickcheck(prop as fn(_, _) -> _)
    }
}
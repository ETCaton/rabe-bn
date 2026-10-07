#[cfg(feature = "borsh")]
use borsh::{BorshDeserialize, BorshSerialize};
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use core::fmt;
use rand::Rng;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::iter::FromIterator;

/// 256-bit, stack allocated biginteger for use in prime field
/// arithmetic.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(C)]
pub struct U256(pub [u64; 4]);

impl From<[u64; 4]> for U256 {
    fn from(d: [u64; 4]) -> Self {
        U256(d)
    }
}

/// 512-bit, stack allocated biginteger for use in extension
/// field serialization and scalar interpretation.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(C)]
pub struct U512(pub [u64; 8]);

impl U512 {
    /// Multiplies c1 by modulo, adds c0.
    #[allow(dead_code)]
    pub fn from(c1: &U256, c0: &U256, modulo: &U256) -> U512 {
        let mut res = [0; 8];

        for (i, xi) in c1.0.iter().enumerate() {
            mac_digit(&mut res[i..], &modulo.0, *xi);
        }

        let mut c0_iter = c0.0.iter();
        let mut carry = 0;

        for ai in res.iter_mut() {
            if let Some(bi) = c0_iter.next() {
                *ai = adc(*ai, *bi, &mut carry);
            } else if carry != 0 {
                *ai = adc(*ai, 0, &mut carry);
            } else {
                break;
            }
        }

        debug_assert!(0 == carry);

        U512(res)
    }

    /// Get a random U512
    pub fn random<R: Rng + ?Sized>(rng: &mut R) -> U512 {
        U512(rng.r#gen())
    }

    pub fn get_bit(&self, n: usize) -> Option<bool> {
        if n >= 512 {
            None
        } else {
            let part = n / 64;
            let bit = n - (64 * part);

            Some(self.0[part] & (1 << bit) > 0)
        }
    }

    /// Divides self by modulo, returning remainder and, if
    /// possible, a quotient smaller than the modulus.
    pub fn divrem(&self, modulo: &U256) -> (Option<U256>, U256) {
        let mut q = Some(U256::zero());
        let mut r = U256::zero();

        for i in (0..512).rev() {
            // NB: modulo's first two bits are always unset
            // so this will never destroy information
            mul2(&mut r.0);
            assert!(r.set_bit(0, self.get_bit(i).unwrap()));
            if &r >= modulo {
                sub_noborrow(&mut r.0, &modulo.0);
                if q.is_some() && !q.as_mut().unwrap().set_bit(i, true) {
                    q = None
                }
            }
        }

        if q.is_some() && (q.as_ref().unwrap() >= modulo) {
            (None, r)
        } else {
            (q, r)
        }
    }

    pub fn interpret(buf: &[u8; 64]) -> U512 {
        let mut n = [0; 8];
        for (l, i) in (0..8).rev().zip((0..8).map(|i| i * 8)) {
            n[l] = BigEndian::read_u64(&buf[i..]);
        }

        U512(n)
    }
}

impl fmt::Display for U512 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut str = String::new();
        for tup in self.0.iter() {
            str.push_str(format!("{:#X?}", tup).as_ref())
        }
        write!(f, "{:?}", str)
    }
}

impl FromIterator<u64> for U512 {
    fn from_iter<I: IntoIterator<Item = u64>>(iter: I) -> Self {
        let mut barry: Vec<u8> = Vec::new();
        for word in iter {
            for v in word.to_le_bytes() {
                barry.push(v)
            }
        }
        let mut array = [0u8; 64];
        for (&x, p) in barry.iter().zip(array.iter_mut()) {
            *p = x;
        }
        U512::interpret(&array)
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut str = String::new();
        for tup in self.0.iter() {
            str.push_str(format!("{:#X?}", tup).as_ref())
        }
        write!(f, "{:?}", str)
    }
}

impl Ord for U256 {
    #[inline]
    fn cmp(&self, other: &U256) -> Ordering {
        for (a, b) in self.0.iter().zip(other.0.iter()).rev() {
            if *a < *b {
                return Ordering::Less;
            } else if *a > *b {
                return Ordering::Greater;
            }
        }

        Ordering::Equal
    }
}

impl PartialOrd for U256 {
    #[inline]
    fn partial_cmp(&self, other: &U256) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// U256/U512 errors
#[derive(Debug)]
pub enum Error {
    InvalidLength { expected: usize, actual: usize },
}

impl U256 {
    /// Initialize U256 from slice of bytes (big endian)
    pub fn from_slice(s: &[u8]) -> Result<U256, Error> {
        if s.len() != 32 {
            return Err(Error::InvalidLength {
                expected: 32,
                actual: s.len(),
            });
        }

        let mut n = [0; 4];
        for (l, i) in (0..4).rev().zip((0..4).map(|i| i * 8)) {
            n[l] = BigEndian::read_u64(&s[i..]);
        }

        Ok(U256(n))
    }
    #[inline]
    pub fn zero() -> U256 {
        U256([0, 0, 0, 0])
    }

    #[inline]
    pub fn one() -> U256 {
        U256([1, 0, 0, 0])
    }

    #[inline]
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut wtr = vec![];
        for elem in self.0.iter().rev().copied() {
            wtr.write_u64::<BigEndian>(elem).unwrap();
        }
        wtr
    }

    /// Produce a random number (mod `modulo`)
    pub fn random<R: Rng + ?Sized>(rng: &mut R, modulo: &U256) -> U256 {
        U512::random(rng).divrem(modulo).1
    }

    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    pub fn set_bit(&mut self, n: usize, to: bool) -> bool {
        if n >= 256 {
            false
        } else {
            let part = n / 64;
            let bit = n - (64 * part);

            if to {
                self.0[part] |= 1 << bit;
            } else {
                self.0[part] &= !(1 << bit);
            }

            true
        }
    }

    pub fn get_bit(&self, n: usize) -> Option<bool> {
        if n >= 256 {
            None
        } else {
            let part = n / 64;
            let bit = n - (64 * part);

            Some(self.0[part] & (1 << bit) > 0)
        }
    }

    /// Add `other` to `self` (mod `modulo`)
    pub fn add(&mut self, other: &U256, modulo: &U256) {
        assert!(
            *self < *modulo && *other < *modulo,
            "operands not below modulus"
        );

        add_nocarry(&mut self.0, &other.0);

        if *self >= *modulo {
            sub_noborrow(&mut self.0, &modulo.0);
        }
    }

    /// Subtract `other` from `self` (mod `modulo`)
    pub fn sub(&mut self, other: &U256, modulo: &U256) {
        assert!(
            *self < *modulo && *other < *modulo,
            "operands not below modulus"
        );

        if *self < *other {
            add_nocarry(&mut self.0, &modulo.0);
        }

        sub_noborrow(&mut self.0, &other.0);
    }

    /// Multiply `self` by `other` (mod `modulo`) via the Montgomery
    /// multiplication method.
    pub fn mul(&mut self, other: &U256, modulo: &U256, inv: u64) {
        assert!(*other < *modulo, "operands not below modulus");

        mul_reduce(&mut self.0, &other.0, &modulo.0, inv);

        if *self >= *modulo {
            sub_noborrow(&mut self.0, &modulo.0);
        }
    }

    /// Turn `self` into its additive inverse (mod `modulo`)
    pub fn neg(&mut self, modulo: &U256) {
        assert!(*self < *modulo, "operands not below modulus");

        if *self > Self::zero() {
            let mut tmp = modulo.0;
            sub_noborrow(&mut tmp, &self.0);

            self.0 = tmp;
        }
    }

    #[inline]
    pub fn is_even(&self) -> bool {
        self.0[0] & 1 == 0
    }

    /// Turn `self` into its multiplicative inverse (mod `modulo`)
    pub fn invert(&mut self, modulo: &U256) {
        // Guajardo Kumar Paar Pelzl
        // Efficient Software-Implementation of Finite Fields with Applications to Cryptography
        // Algorithm 16 (BEA for Inversion in Fp)
        assert!(*self < *modulo, "operands not below modulus");

        let mut u = *self;
        let mut v = *modulo;
        let mut b = U256::one();
        let mut c = U256::zero();

        while u != U256::one() && v != U256::one() {
            while u.is_even() {
                div2(&mut u.0);

                if b.is_even() {
                    div2(&mut b.0);
                } else {
                    add_nocarry(&mut b.0, &modulo.0);
                    div2(&mut b.0);
                }
            }
            while v.is_even() {
                div2(&mut v.0);

                if c.is_even() {
                    div2(&mut c.0);
                } else {
                    add_nocarry(&mut c.0, &modulo.0);
                    div2(&mut c.0);
                }
            }

            if u >= v {
                sub_noborrow(&mut u.0, &v.0);
                b.sub(&c, modulo);
            } else {
                sub_noborrow(&mut v.0, &u.0);
                c.sub(&b, modulo);
            }
        }

        if u == U256::one() {
            self.0 = b.0;
        } else {
            self.0 = c.0;
        }
    }

    /// Return an Iterator<Item=bool> over all bits from
    /// MSB to LSB.
    pub fn bits(&self) -> BitIterator<'_> {
        BitIterator { int: self, n: 256 }
    }
}

pub struct BitIterator<'a> {
    int: &'a U256,
    n: usize,
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;

            self.int.get_bit(self.n)
        }
    }
}

/// Divide by two
#[inline]
fn div2(a: &mut [u64; 4]) {
    let mut t = a[3] << 63;
    a[3] >>= 1;
    let b = a[2] << 63;
    a[2] >>= 1;
    a[2] |= t;
    t = a[1] << 63;
    a[1] >>= 1;
    a[1] |= b;
    a[0] >>= 1;
    a[0] |= t;
}

/// Multiply by two
#[inline]
fn mul2(a: &mut [u64; 4]) {
    let mut last = 0;
    for i in a {
        let tmp = *i >> 63;
        *i <<= 1;
        *i |= last;
        last = tmp;
    }
}

#[inline(always)]
fn split_u64(i: u64) -> (u64, u64) {
    (i >> 32, i & 0xFFFFFFFF)
}

#[inline(always)]
fn combine_u64(hi: u64, lo: u64) -> u64 {
    (hi << 32) | lo
}

#[inline]
fn adc(a: u64, b: u64, carry: &mut u64) -> u64 {
    let (a1, a0) = split_u64(a);
    let (b1, b0) = split_u64(b);
    let (c, r0) = split_u64(a0 + b0 + *carry);
    let (c, r1) = split_u64(a1 + b1 + c);
    *carry = c;

    combine_u64(r1, r0)
}

#[inline]
fn add_nocarry(a: &mut [u64; 4], b: &[u64; 4]) {
    let mut carry = 0;

    for (a, b) in a.iter_mut().zip(b.iter()) {
        *a = adc(*a, *b, &mut carry);
    }

    debug_assert!(0 == carry);
}

#[inline]
fn sub_noborrow(a: &mut [u64; 4], b: &[u64; 4]) {
    #[inline]
    fn sbb(a: u64, b: u64, borrow: &mut u64) -> u64 {
        let (a1, a0) = split_u64(a);
        let (b1, b0) = split_u64(b);
        let (b, r0) = split_u64((1 << 32) + a0 - b0 - *borrow);
        let (b, r1) = split_u64((1 << 32) + a1 - b1 - ((b == 0) as u64));

        *borrow = (b == 0) as u64;

        combine_u64(r1, r0)
    }

    let mut borrow = 0;

    for (a, b) in a.iter_mut().zip(b.iter()) {
        *a = sbb(*a, *b, &mut borrow);
    }

    debug_assert!(0 == borrow);
}

fn mac_digit(acc: &mut [u64], b: &[u64], c: u64) {
    #[inline]
    fn mac_with_carry(a: u64, b: u64, c: u64, carry: &mut u64) -> u64 {
        let (b_hi, b_lo) = split_u64(b);
        let (c_hi, c_lo) = split_u64(c);

        let (a_hi, a_lo) = split_u64(a);
        let (carry_hi, carry_lo) = split_u64(*carry);
        let (x_hi, x_lo) = split_u64(b_lo * c_lo + a_lo + carry_lo);
        let (y_hi, y_lo) = split_u64(b_lo * c_hi);
        let (z_hi, z_lo) = split_u64(b_hi * c_lo);
        let (r_hi, r_lo) = split_u64(x_hi + y_lo + z_lo + a_hi + carry_hi);

        *carry = (b_hi * c_hi) + r_hi + y_hi + z_hi;

        combine_u64(r_lo, x_lo)
    }

    if c == 0 {
        return;
    }

    let mut b_iter = b.iter();
    let mut carry = 0;

    for ai in acc.iter_mut() {
        if let Some(bi) = b_iter.next() {
            *ai = mac_with_carry(*ai, *bi, c, &mut carry);
        } else if carry != 0 {
            *ai = mac_with_carry(*ai, 0, c, &mut carry);
        } else {
            break;
        }
    }

    debug_assert!(carry == 0);
}

fn mul_reduce(this: &mut [u64; 4], by: &[u64; 4], modulus: &[u64; 4], inv: u64) {
    // The Montgomery reduction here is based on Algorithm 14.32 in
    // Handbook of Applied Cryptography
    // <http://cacr.uwaterloo.ca/hac/about/chap14.pdf>.

    let mut res = [0; 2 * 4];
    for (i, xi) in this.iter().enumerate() {
        mac_digit(&mut res[i..], by, *xi);
    }

    for i in 0..4 {
        let k = inv.wrapping_mul(res[i]);
        mac_digit(&mut res[i..], modulus, k);
    }

    this.copy_from_slice(&res[4..]);
}

#[cfg(test)]
mod test {
    #[cfg(feature = "borsh")]
    extern crate borsh;
    #[cfg(feature = "serde")]
    extern crate serde_json;
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::{Fr, G1, Group};
    #[cfg(feature = "serde")]
    use crate::{G2, Gt};

    #[test]
    fn setting_bits() {
        let rng = &mut ::rand::thread_rng();
        let modulo = U256([0xffffffffffffffff; 4]);

        let a = U256::random(rng, &modulo);
        let mut e = U256::zero();
        for (i, b) in a.bits().enumerate() {
            assert!(e.set_bit(255 - i, b));
        }

        assert_eq!(a, e);
    }

    #[test]
    fn testing_divrem() {
        let rng = &mut ::rand::thread_rng();

        let modulo = U256([
            0x3c208c16d87cfd47,
            0x97816a916871ca8d,
            0xb85045b68181585d,
            0x30644e72e131a029,
        ]);

        for _ in 0..100 {
            let c0 = U256::random(rng, &modulo);
            let c1 = U256::random(rng, &modulo);

            let c1q_plus_c0 = U512::from(&c1, &c0, &modulo);

            let (new_c1, new_c0) = c1q_plus_c0.divrem(&modulo);

            assert_eq!(c1, new_c1.unwrap());
            assert_eq!(c0, new_c0);
        }

        {
            // Modulus should become 1*q + 0
            let a = U512([
                0x3c208c16d87cfd47,
                0x97816a916871ca8d,
                0xb85045b68181585d,
                0x30644e72e131a029,
                0,
                0,
                0,
                0,
            ]);

            let (c1, c0) = a.divrem(&modulo);
            assert_eq!(c1.unwrap(), U256::one());
            assert_eq!(c0, U256::zero());
        }

        {
            // Modulus squared minus 1 should be (q-1) q + q-1
            let a = U512([
                0x3b5458a2275d69b0,
                0xa602072d09eac101,
                0x4a50189c6d96cadc,
                0x04689e957a1242c8,
                0x26edfa5c34c6b38d,
                0xb00b855116375606,
                0x599a6f7c0348d21c,
                0x0925c4b8763cbf9c,
            ]);

            let (c1, c0) = a.divrem(&modulo);
            assert_eq!(
                c1.unwrap(),
                U256([
                    0x3c208c16d87cfd46,
                    0x97816a916871ca8d,
                    0xb85045b68181585d,
                    0x30644e72e131a029,
                ],)
            );
            assert_eq!(
                c0,
                U256([
                    0x3c208c16d87cfd46,
                    0x97816a916871ca8d,
                    0xb85045b68181585d,
                    0x30644e72e131a029,
                ],)
            );
        }

        {
            // Modulus squared minus 2 should be (q-1) q + q-2
            let a = U512([
                0x3b5458a2275d69af,
                0xa602072d09eac101,
                0x4a50189c6d96cadc,
                0x04689e957a1242c8,
                0x26edfa5c34c6b38d,
                0xb00b855116375606,
                0x599a6f7c0348d21c,
                0x0925c4b8763cbf9c,
            ]);

            let (c1, c0) = a.divrem(&modulo);

            assert_eq!(
                c1.unwrap(),
                U256([
                    0x3c208c16d87cfd46,
                    0x97816a916871ca8d,
                    0xb85045b68181585d,
                    0x30644e72e131a029,
                ],)
            );
            assert_eq!(
                c0,
                U256([
                    0x3c208c16d87cfd45,
                    0x97816a916871ca8d,
                    0xb85045b68181585d,
                    0x30644e72e131a029,
                ],)
            );
        }

        {
            // Ridiculously large number should fail
            let a = U512([
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
            ]);

            let (c1, c0) = a.divrem(&modulo);
            assert!(c1.is_none());
            assert_eq!(
                c0,
                U256([
                    0xf32cfc5b538afa88,
                    0xb5e71911d44501fb,
                    0x47ab1eff0a417ff6,
                    0x06d89f71cab8351f,
                ],)
            );
        }

        {
            // Modulus squared should fail
            let a = U512([
                0x3b5458a2275d69b1,
                0xa602072d09eac101,
                0x4a50189c6d96cadc,
                0x04689e957a1242c8,
                0x26edfa5c34c6b38d,
                0xb00b855116375606,
                0x599a6f7c0348d21c,
                0x0925c4b8763cbf9c,
            ]);

            let (c1, c0) = a.divrem(&modulo);
            assert!(c1.is_none());
            assert_eq!(c0, U256::zero());
        }

        {
            // Modulus squared plus one should fail
            let a = U512([
                0x3b5458a2275d69b2,
                0xa602072d09eac101,
                0x4a50189c6d96cadc,
                0x04689e957a1242c8,
                0x26edfa5c34c6b38d,
                0xb00b855116375606,
                0x599a6f7c0348d21c,
                0x0925c4b8763cbf9c,
            ]);

            let (c1, c0) = a.divrem(&modulo);
            assert!(c1.is_none());
            assert_eq!(c0, U256::one());
        }

        {
            let modulo = U256([
                0x43e1f593f0000001,
                0x2833e84879b97091,
                0xb85045b68181585d,
                0x30644e72e131a029,
            ]);

            // Fr modulus masked off is valid
            let a = U512([
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0xffffffffffffffff,
                0x07ffffffffffffff,
            ]);

            let (c1, c0) = a.divrem(&modulo);

            assert!(c1.unwrap() < modulo);
            assert!(c0 < modulo);
        }
    }

    ///`debug_assert!(0 == carry)` in the `add_nocarry` function assumed both operands are already
    /// reduced below modulus, but does not check or enforce it. Debug builds tripped the assert,
    /// but release ones overflowed and wrapped around, returning incorrect values
    #[test]
    #[should_panic(expected = "operands not below modulus")]
    fn add_nocarry_assert_fires_for_overflowing_operands() {
        let mut a = U256([u64::MAX; 4]);
        let b = U256([1, 0, 0, 0]);
        let modulo = U256([u64::MAX; 4]);

        a.add(&b, &modulo);
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn fr_borsh_rejects_values_above_modulus() {
        let bytes = [0xffu8; 32];
        assert!(borsh::from_slice::<Fr>(&bytes).is_err())
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn fr_borsh_round_trips_values_below_modulus() {
        let x = Fr::one() + Fr::one();
        let bytes = borsh::to_vec(&x).unwrap();
        assert!(borsh::from_slice::<Fr>(&bytes).unwrap() == x);
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn g1_borsh_rejects_off_curve_point() {
        use crate::G1;

        // x = y = z = 1 would not be on the curve
        let bytes = [1u64, 0, 0, 0]
            .repeat(3)
            .into_iter()
            .flat_map(|limb| limb.to_le_bytes())
            .collect::<Vec<_>>();

        assert!(borsh::from_slice::<G1>(&bytes).is_err());
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn g2_borsh_round_trips_coordinates() {
        use crate::{G2, Group};

        let p = G2::one() + G2::one();
        assert!(
            borsh::to_vec(&p)
                .map(|bytes| borsh::from_slice::<G2>(&bytes))
                .flatten()
                .is_ok_and(|bytes| bytes == p)
        );
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn g2_borsh_rejects_flipped_coordinate() {
        use crate::{G2, Group};

        let p = G2::one() + G2::one();
        let mut bytes = borsh::to_vec(&p).unwrap();
        bytes[0] ^= 1;
        assert!(borsh::from_slice::<G2>(&bytes).is_err());
    }

    #[test]
    #[should_panic(expected = "operands not below modulus")]
    fn sub_rejects_unreduced_operands() {
        let mut a = U256([1, 0, 0, 0]);
        let b = U256([u64::MAX; 4]);
        let modulo = U256([7, 0, 0, 0]);
        a.sub(&b, &modulo);
    }

    #[test]
    #[should_panic(expected = "operands not below modulus")]
    fn mul_rejects_unreduced_operands() {
        let mut a = U256([1, 0, 0, 0]);
        let b = U256([u64::MAX; 4]);
        let modulo = U256([7, 0, 0, 0]);
        a.mul(&b, &modulo, 1);
    }

    #[test]
    #[should_panic(expected = "operands not below modulus")]
    fn neg_rejects_unreduced_operands() {
        let mut a = U256([u64::MAX; 4]);
        let modulo = U256([7, 0, 0, 0]);
        a.neg(&modulo);
    }

    #[test]
    fn reduced_operands_are_accepted() {
        let modulo = U256([7, 0, 0, 0]);
        let mut a = U256([5, 0, 0, 0]);
        a.add(&U256([4, 0, 0, 0]), &modulo);
        a.sub(&U256([3, 0, 0, 0]), &modulo);
        assert_eq!(a, U256([6, 0, 0, 0]));
        a.neg(&modulo);
        assert_eq!(a, U256([1, 0, 0, 0]));
    }

    /// `Fr` us `U256` limbs in Montgomery form, but nothing checked if thevalue is below the
    /// modulus, so 2^256 - 1 was accepted
    #[test]
    #[cfg(feature = "serde")]
    fn fr_deserialize_rejects_value_above_modulus() {
        let all_ones: &str =
            "[18446744073709551615,18446744073709551615,18446744073709551615,18446744073709551615]";
        let fr: Result<Fr, _> = serde_json::from_str(all_ones);
        let err = fr
            .expect_err("Invalid (above modulus) Fr accepted")
            .to_string();
        assert!(err.contains("not below modulus"), "{}", err);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn fr_deserialize_boundary_values() {
        let r: Vec<u64> = vec![
            4891460686036598785,
            2896914383306846353,
            13281191951274694749,
            3486998266802970665,
        ];
        let r_minus_1 = r.iter().map(|e| e - 1).collect::<Vec<_>>();

        assert!(serde_json::from_str::<Fr>(&format!("{:?}", r)).is_err());
        assert!(serde_json::from_str::<Fr>(&format!("{:?}", r_minus_1)).is_ok());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn fr_round_trips_values_below_modulus() {
        let mut rng = StdRng::seed_from_u64(250);
        for _ in 0..200 {
            let x: Fr = rng.r#gen();
            assert!(
                serde_json::to_string(&x)
                    .map(|s| serde_json::from_str::<Fr>(&s))
                    .flatten()
                    .is_ok_and(|back| back == x)
            );
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn g1_deserialize_rejects_coordinates_above_modulus() {
        let json = format!(
            r#"{{"x":[4332616871279656263,10917124144477883021,13281191951274694749,3486998266802970665],"y":[1,0,0,0],"z":[1,0,0,0]}}"#
        );
        assert!(
            serde_json::from_str::<G1>(&json)
                .is_err_and(|err| err.to_string().contains("not below modulus"))
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn g1_deserialize_rejects_off_curve_point() {
        let json = r#"{"x":[1,0,0,0],"y":[1,0,0,0],"z":[1,0,0,0]}"#;
        let g: Result<G1, _> = serde_json::from_str(json);
        assert!(
            g.is_err_and(|err| err.to_string().contains("not on the curve")),
            "off-curve G1 point was accepted"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn g2_deserialize_rejects_off_curve_point() {
        let fq2 = r#"{"c0":[1,0,0,0],"c1":[0,0,0,0]}"#;
        let json = format!(r#"{{"x":{fq2},"y":{fq2},"z":{fq2}}}"#);
        let g: Result<G2, _> = serde_json::from_str(&json);
        assert!(
            g.is_err_and(|err| err.to_string().contains("not on the curve")),
            "off-curve G2 point was accepted"
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn curve_points_round_trip_via_serde() {
        let mut rng = StdRng::seed_from_u64(250);
        for _ in 0..3 {
            let a: G1 = rng.r#gen();
            let b: G2 = rng.r#gen();
            assert!(
                serde_json::to_string(&a)
                    .map(|s| serde_json::from_str::<G1>(&s))
                    .flatten()
                    .is_ok_and(|serde_a| serde_a == a)
            );
            assert!(
                serde_json::to_string(&b)
                    .map(|s| serde_json::from_str::<G2>(&s))
                    .flatten()
                    .is_ok_and(|serde_b| serde_b == b)
            )
        }

        let z1 = G1::zero();
        let z2 = G2::zero();
        assert!(
            dbg!(
                serde_json::to_string(&z1)
                    .map(|s| serde_json::from_str::<G1>(&s))
                    .flatten()
            )
            .is_ok_and(|z1| z1.is_zero())
        );
        assert!(
            serde_json::to_string(&z2)
                .map(|s| serde_json::from_str::<G2>(&s))
                .flatten()
                .is_ok_and(|z2| z2.is_zero())
        );
    }

    /// `P + (-P)` can leave stale `x` and `y` values with `z == 0` (the identity) which needs to
    /// round trip in its canonical form
    #[test]
    #[cfg(feature = "serde")]
    fn identity_round_trips_and_stays_canonical() {
        let mut rng = StdRng::seed_from_u64(250);
        let p: G1 = rng.r#gen();
        let stale = p + (-p);
        assert!(stale.is_zero());
        assert!(
            serde_json::to_string(&stale)
                .map(|s| serde_json::from_str::<G1>(&s))
                .flatten()
                .is_ok_and(|back| back.is_zero() && back.into_bytes() == G1::zero().into_bytes())
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gt_rejects_zero_element() {
        let fq2 = r#"{"c0":[1,0,0,0],"c1":[0,0,0,0]}"#;
        let fq6 = format!(r#"{{"c0":{fq2},"c1":{fq2},"c2":{fq2}}}"#);
        let json = format!(r#"{{"c0":{fq6},"c1":{fq6}}}"#);
        assert!(
            serde_json::from_str::<Gt>(&json)
                .is_err_and(|err| err.to_string().contains("not in order-r subgroup"))
        )
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gt_deserialize_rejects_elements_outside_subgroup() {
        let mut rng = StdRng::seed_from_u64(250);
        let g: Gt = rng.r#gen();
        assert!(
            (serde_json::to_value(&g)
                .and_then(|mut value| {
                    value["c0"]["c0"]["c0"] = serde_json::json!([1, 2, 3, 4]);
                    Ok(value)
                })
                .map(|json| serde_json::from_value::<Gt>(json)))
            .flatten()
            .is_err_and(|err| err.to_string().contains("not in order-r subgroup"))
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gt_round_trips() {
        let mut rng = StdRng::seed_from_u64(250);
        let g: Gt = rng.r#gen();
        assert!(
            serde_json::to_string(&g)
                .map(|s| serde_json::from_str::<Gt>(&s))
                .flatten()
                .is_ok_and(|back| back == g)
        )
    }

    #[test]
    #[cfg(feature = "borsh")]
    fn gt_borsh_rejects_zero_element() {
        use crate::Gt;
        let bytes = vec![0u8; 12 * 32];
        assert!(borsh::from_slice::<Gt>(&bytes).is_err());
    }

    #[test]
    fn fr_bytes_round_trip_successfully() {
        let mut rng = StdRng::seed_from_u64(250);
        for _ in 0..200 {
            let x: Fr = rng.r#gen();
            let bytes = x.into_bytes();
            assert_eq!(bytes.len(), 32);
            assert!(Fr::from_slice(&bytes).unwrap() == x);
        }
        assert_eq!(Fr::one().into_bytes()[31], 1);
        assert!(Fr::zero().into_bytes().iter().all(|b| *b == 0));
    }

    #[test]
    fn u256_bytes_ruond_trip() {
        let n = U256([1, 2, 3, 4]);
        assert!(U256::from_slice(&n.into_bytes()).is_ok_and(|val| val == n));
    }

    /// `mul` has a final (conditional) subtraction, and so it results in a reduced value and does
    /// not trigger the assert.
    #[test]
    fn ordinary_field_arithmetic_does_not_trip_assert() {
        let mut rng = StdRng::seed_from_u64(250);
        let mut rng = StdRng::seed_from_u64(250);
        let mut acc: Fr = rng.r#gen();
        for _ in 0..200_000 {
            let y: Fr = rng.r#gen();
            acc = acc * y + y - acc;
        }
        let mut g = G1::one();
        for _ in 0..200 {
            let k: Fr = rng.r#gen();
            g = g * k + G1::one();
        }
    }
}

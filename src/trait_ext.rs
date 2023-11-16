use std::{ops::Deref, process::ExitCode, slice::Split};

pub trait ExtendFromSliceUnchecked<T> {
    /// # Safety
    ///
    /// This function requires the caller to uphold the safety contract where the Vec's capacity is
    /// over the sum of its current length and the length of the slice to be extended from.
    unsafe fn extend_from_slice_unchecked(&mut self, slice: &[T]);
}

impl<T> ExtendFromSliceUnchecked<T> for Vec<T> {
    #[inline]
    unsafe fn extend_from_slice_unchecked(&mut self, slice: &[T]) {
        let len = self.len();
        let amt = slice.len();

        std::ptr::copy_nonoverlapping(slice.as_ptr(), self.as_mut_ptr().add(len), amt);
        self.set_len(len + amt);
    }
}

pub trait FromBool {
    fn from_bool(b: bool) -> Self;
}

impl FromBool for ExitCode {
    #[inline]
    fn from_bool(b: bool) -> Self {
        if b {
            Self::SUCCESS
        } else {
            Self::FAILURE
        }
    }
}

pub trait IsWhitespace {
    fn is_whitespace(&self) -> bool;
}

impl IsWhitespace for u8 {
    #[inline]
    fn is_whitespace(&self) -> bool {
        matches!(self, b' ' | b'\x09'..=b'\x0d') || *self > b'\x7f'
    }
}

pub trait Words {
    #[inline]
    fn words<T>(&self) -> Split<T, fn(&T) -> bool>
    where
        T: IsWhitespace,
        Self: Deref<Target = [T]> + Sized,
    {
        self.split(IsWhitespace::is_whitespace)
    }
}

impl<T> Words for T {}

pub trait Bitflag {
    type Index;

    fn bit(&self, pos: Self::Index) -> bool;
    fn set_bit(&mut self, pos: Self::Index);
    fn unset_bit(&mut self, pos: Self::Index);
}

pub struct CapacityOverflow;

pub trait ReserveTotal {
    fn reserve_total(&mut self, total: usize) -> Result<(), CapacityOverflow>;
}

impl<T> ReserveTotal for Vec<T> {
    fn reserve_total(&mut self, total: usize) -> Result<(), CapacityOverflow> {
        if total > self.len() {
            return self
                .try_reserve_exact(total - self.len())
                .map_err(|_| CapacityOverflow);
        }
        Ok(())
    }
}

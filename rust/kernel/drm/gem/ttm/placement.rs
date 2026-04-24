// SPDX-License-Identifier: GPL-2.0 OR MIT

//! TODO docs.

use core::ops::Range;

use crate::prelude::*;
use crate::{
    bindings,
    impl_flags, //
};

/// TODO docs.
///
/// Place types are extensible by each driver, so use a newtype instead of an enum.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PlaceType(u32);

impl PlaceType {
    /// TODO docs.
    pub const SYSTEM: Self = Self(bindings::TTM_PL_SYSTEM);
    /// TODO docs.
    pub const TT: Self = Self(bindings::TTM_PL_TT);
    /// TODO docs.
    pub const VRAM: Self = Self(bindings::TTM_PL_VRAM);
    /// TODO docs.
    pub const PRIV: Self = Self(bindings::TTM_PL_PRIV);

    /// TODO docs.
    pub const fn driver_place_type<const V: u32>() -> Self {
        const { assert!(V >= bindings::TTM_PL_PRIV && V < bindings::TTM_NUM_MEM_TYPES) };
        Self(V)
    }

    fn as_raw(self) -> u32 {
        self.0
    }
}

impl_flags!(
    /// TODO docs.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PlaceFlags(u32);

    /// TODO docs.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum PlaceFlag {
        /// TODO docs.
        Contiguous = bindings::TTM_PL_FLAG_CONTIGUOUS,
        /// TODO docs.
        TopDown = bindings::TTM_PL_FLAG_TOPDOWN,
        /// TODO docs.
        Temporary = bindings::TTM_PL_FLAG_TEMPORARY,
        /// TODO docs.
        Desired = bindings::TTM_PL_FLAG_DESIRED,
        /// TODO docs.
        Fallback = bindings::TTM_PL_FLAG_FALLBACK,
    }
);

/// TODO docs.
pub struct PfnRange {
    fpfn: u32,
    lpfn: u32,
}

impl PfnRange {
    /// TODO: docs.
    pub const ALL: Self = Self { fpfn: 0, lpfn: 0 };

    /// TODO docs.
    pub const fn new(range: Range<u32>) -> Result<Self> {
        if range.start >= range.end {
            return Err(EINVAL);
        }

        Ok(Self {
            fpfn: range.start,
            lpfn: range.end,
        })
    }

    /// TODO docs.
    pub const fn from_start(fpfn: u32) -> Self {
        Self { fpfn, lpfn: 0 }
    }
}

/// TODO docs.
#[repr(transparent)]
pub struct Place(bindings::ttm_place);

impl Place {
    /// TODO docs.
    pub fn new(range: PfnRange, place_type: PlaceType, flags: PlaceFlags) -> Self {
        Self(bindings::ttm_place {
            fpfn: range.fpfn,
            lpfn: range.lpfn,
            mem_type: place_type.as_raw(),
            flags: flags.into(),
        })
    }
}

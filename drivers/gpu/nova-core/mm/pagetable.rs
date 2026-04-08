// SPDX-License-Identifier: GPL-2.0

//! Common page table types shared between MMU v2 and v3.
//!
//! This module provides foundational types used by both MMU versions:
//! - Page table level hierarchy
//! - Memory aperture types for PDEs and PTEs

pub(crate) mod ver2;
pub(crate) mod ver3;
pub(crate) mod walk;

use crate::gpu::Architecture;
use crate::mm::{
    pramin,
    Pfn,
    VirtualAddress,
    VramAddress, //
};
use kernel::prelude::*;

/// Extracts the page table index at a given level from a virtual address.
pub(crate) trait VaLevelIndex {
    /// Return the page table index at `level` for this virtual address.
    fn level_index(&self, level: PageTableLevel) -> u64;
}

/// MMU-specific page table operations.
pub(crate) trait Mmu {
    /// Virtual address layout type for this MMU.
    type Va: VaLevelIndex;
    /// Page table entry type for this MMU.
    type Pte: PteOps;
    /// Page directory entry type for this MMU.
    type Pde: PdeOps;
    /// Dual page directory entry type for this MMU.
    type DualPde: DualPdeOps;

    /// `PDE` levels (excluding `PTE` level) for page table walking.
    const PDE_LEVELS: &'static [PageTableLevel];
    /// `PTE` level for this MMU.
    const PTE_LEVEL: PageTableLevel;
    /// Dual `PDE` level (128-bit entries) for this MMU.
    const DUAL_PDE_LEVEL: PageTableLevel;

    /// Creates the MMU-specific virtual address view.
    fn va(va: VirtualAddress) -> Self::Va;

    /// Returns the number of entries per page at `level`.
    fn entries_per_page(level: PageTableLevel) -> usize;

    /// Returns the entry size in bytes for `level`.
    fn entry_size(level: PageTableLevel) -> usize {
        if level == Self::DUAL_PDE_LEVEL {
            16
        } else {
            8
        }
    }

    /// Returns the number of `PDE` levels for this MMU.
    fn pde_level_count() -> usize {
        Self::PDE_LEVELS.len()
    }

    /// Computes an upper bound on page table pages needed for `num_virt_pages`.
    fn pt_pages_upper_bound(num_virt_pages: usize) -> usize {
        let mut total = 0;

        let pte_epp = Self::entries_per_page(Self::PTE_LEVEL);
        let mut pages_at_level = num_virt_pages.div_ceil(pte_epp);
        total += pages_at_level;

        for &level in Self::PDE_LEVELS.iter().rev() {
            let epp = Self::entries_per_page(level);
            pages_at_level = pages_at_level.div_ceil(epp);
            total += pages_at_level;
        }

        total
    }
}

/// Common `PTE` operations shared by MMU versions.
pub(crate) trait PteOps: Sized {
    /// Creates a `PTE` from a raw `u64` value.
    fn new(val: u64) -> Self;

    /// Creates an invalid `PTE`.
    fn invalid() -> Self;

    /// Creates a valid VRAM-backed `PTE`.
    fn new_vram(pfn: Pfn, writable: bool) -> Self;

    /// Returns whether this `PTE` is valid.
    fn is_valid(&self) -> bool;

    /// Returns the physical frame number.
    fn frame_number(&self) -> Pfn;

    /// Returns the raw `u64` value.
    fn raw_u64(&self) -> u64;

    /// Reads a `PTE` from VRAM.
    fn read(window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result<Self> {
        Ok(Self::new(window.try_read64(addr.raw())?))
    }

    /// Writes this `PTE` to VRAM.
    fn write(&self, window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result {
        window.try_write64(addr.raw(), self.raw_u64())
    }
}

/// Common `PDE` operations shared by MMU versions.
pub(crate) trait PdeOps: Sized {
    /// Creates a `PDE` from a raw `u64` value.
    fn new(val: u64) -> Self;

    /// Creates a valid VRAM-backed `PDE`.
    fn new_vram(table_pfn: Pfn) -> Self;

    /// Returns whether this `PDE` is valid.
    fn is_valid(&self) -> bool;

    /// Returns the memory aperture of this `PDE`.
    fn aperture(&self) -> AperturePde;

    /// Returns the frame number of the next-level table.
    fn table_frame(&self) -> Pfn;

    /// Returns the VRAM address of the next-level table.
    fn table_vram_address(&self) -> VramAddress;

    /// Returns the raw `u64` value.
    fn raw_u64(&self) -> u64;

    /// Reads a `PDE` from VRAM.
    fn read(window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result<Self> {
        Ok(Self::new(window.try_read64(addr.raw())?))
    }

    /// Writes this `PDE` to VRAM.
    fn write(&self, window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result {
        window.try_write64(addr.raw(), self.raw_u64())
    }
}

/// Common dual `PDE` operations shared by MMU versions.
pub(crate) trait DualPdeOps: Sized {
    /// Creates a dual `PDE` from raw 128-bit value.
    fn new(big: u64, small: u64) -> Self;

    /// Creates a dual `PDE` with only the small page table pointer set.
    fn new_small(table_pfn: Pfn) -> Self;

    /// Returns whether the small page table pointer is valid.
    fn has_small(&self) -> bool;

    /// Returns whether the big page table pointer is valid.
    fn has_big(&self) -> bool;

    /// Returns the frame number of the small page table pointer.
    fn small_pfn(&self) -> Pfn;

    /// Returns the VRAM address of the small page table.
    fn small_vram_address(&self) -> VramAddress;

    /// Returns the raw `u64` value of the big word.
    fn big_raw_u64(&self) -> u64;

    /// Returns the raw `u64` value of the small word.
    fn small_raw_u64(&self) -> u64;

    /// Reads a dual `PDE` from VRAM.
    fn read(window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result<Self> {
        let lo = window.try_read64(addr.raw())?;
        let hi = window.try_read64(addr.raw() + 8)?;
        Ok(Self::new(lo, hi))
    }

    /// Writes this dual `PDE` to VRAM.
    fn write(&self, window: &mut pramin::PraminWindow<'_>, addr: VramAddress) -> Result {
        window.try_write64(addr.raw(), self.big_raw_u64())?;
        window.try_write64(addr.raw() + 8, self.small_raw_u64())
    }
}

/// MMU version enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmuVersion {
    /// MMU v2 for Turing/Ampere/Ada.
    V2,
    /// MMU v3 for Hopper and later.
    V3,
}

impl From<Architecture> for MmuVersion {
    fn from(arch: Architecture) -> Self {
        match arch {
            Architecture::Turing | Architecture::Ampere | Architecture::Ada => Self::V2,
            // In the future, uncomment the following to support V3.
            // _ => Self::V3,
        }
    }
}

/// Page Table Level hierarchy for MMU v2/v3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageTableLevel {
    /// Level 0 - Page Directory Base (root).
    Pdb,
    /// Level 1 - Intermediate page directory.
    L1,
    /// Level 2 - Intermediate page directory.
    L2,
    /// Level 3 - Intermediate page directory or dual PDE (version-dependent).
    L3,
    /// Level 4 - PTE level for v2, intermediate page directory for v3.
    L4,
    /// Level 5 - PTE level used for MMU v3 only.
    L5,
}

/// Memory aperture for Page Table Entries (`PTE`s).
///
/// Determines which memory region the `PTE` points to.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AperturePte {
    /// Local video memory (VRAM).
    #[default]
    VideoMemory = 0,
    /// Peer GPU's video memory.
    PeerMemory = 1,
    /// System memory with cache coherence.
    SystemCoherent = 2,
    /// System memory without cache coherence.
    SystemNonCoherent = 3,
}

// TODO[FPRI]: Replace with `#[derive(FromPrimitive)]` when available.
impl From<u8> for AperturePte {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::VideoMemory,
            1 => Self::PeerMemory,
            2 => Self::SystemCoherent,
            3 => Self::SystemNonCoherent,
            _ => Self::VideoMemory,
        }
    }
}

// TODO[FPRI]: Replace with `#[derive(ToPrimitive)]` when available.
impl From<AperturePte> for u8 {
    fn from(val: AperturePte) -> Self {
        val as u8
    }
}

/// Memory aperture for Page Directory Entries (`PDE`s).
///
/// Note: For `PDE`s, `Invalid` (0) means the entry is not valid.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AperturePde {
    /// Invalid/unused entry.
    #[default]
    Invalid = 0,
    /// Page table is in video memory.
    VideoMemory = 1,
    /// Page table is in system memory with coherence.
    SystemCoherent = 2,
    /// Page table is in system memory without coherence.
    SystemNonCoherent = 3,
}

// TODO[FPRI]: Replace with `#[derive(FromPrimitive)]` when available.
impl From<u8> for AperturePde {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::VideoMemory,
            2 => Self::SystemCoherent,
            3 => Self::SystemNonCoherent,
            _ => Self::Invalid,
        }
    }
}

// TODO[FPRI]: Replace with `#[derive(ToPrimitive)]` when available.
impl From<AperturePde> for u8 {
    fn from(val: AperturePde) -> Self {
        val as u8
    }
}

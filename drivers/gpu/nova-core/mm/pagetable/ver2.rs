// SPDX-License-Identifier: GPL-2.0

//! MMU v2 page table types for Turing and Ampere GPUs.
//!
//! This module defines MMU version 2 specific types (Turing, Ampere and Ada GPUs).
//!
//! Bit field layouts derived from the NVIDIA OpenRM documentation:
//! `open-gpu-kernel-modules/src/common/inc/swref/published/turing/tu102/dev_mmu.h`

use super::{
    AperturePde,
    AperturePte,
    DualPdeOps,
    Mmu,
    PageTableLevel,
    PdeOps,
    PteOps,
    VaLevelIndex, //
};
use crate::mm::{
    Pfn,
    VirtualAddress,
    VramAddress, //
};

bitfield! {
    pub(crate) struct VirtualAddressV2(u64), "MMU v2 49-bit virtual address layout" {
        11:0    offset   as u64, "Page offset [11:0]";
        20:12   pt_idx   as u64, "PT index [20:12]";
        28:21   pde0_idx as u64, "PDE0 index [28:21]";
        37:29   pde1_idx as u64, "PDE1 index [37:29]";
        46:38   pde2_idx as u64, "PDE2 index [46:38]";
        48:47   pde3_idx as u64, "PDE3 index [48:47]";
    }
}

impl VirtualAddressV2 {
    /// Create a [`VirtualAddressV2`] from a [`VirtualAddress`].
    pub(crate) fn new(va: VirtualAddress) -> Self {
        Self(va.raw_u64())
    }
}

impl VaLevelIndex for VirtualAddressV2 {
    fn level_index(&self, level: PageTableLevel) -> u64 {
        match level {
            PageTableLevel::Pdb => self.pde3_idx(),
            PageTableLevel::L1 => self.pde2_idx(),
            PageTableLevel::L2 => self.pde1_idx(),
            PageTableLevel::L3 => self.pde0_idx(),
            PageTableLevel::L4 => self.pt_idx(),
            PageTableLevel::L5 => 0,
        }
    }
}

/// MMU v2 marker type.
pub(crate) struct MmuV2;

impl Mmu for MmuV2 {
    type Va = VirtualAddressV2;
    type Pte = Pte;
    type Pde = Pde;
    type DualPde = DualPde;

    const PDE_LEVELS: &'static [PageTableLevel] = &[
        PageTableLevel::Pdb,
        PageTableLevel::L1,
        PageTableLevel::L2,
        PageTableLevel::L3,
    ];
    const PTE_LEVEL: PageTableLevel = PageTableLevel::L4;
    const DUAL_PDE_LEVEL: PageTableLevel = PageTableLevel::L3;

    fn va(va: VirtualAddress) -> Self::Va {
        VirtualAddressV2::new(va)
    }

    fn entries_per_page(level: PageTableLevel) -> usize {
        match level {
            PageTableLevel::Pdb => 4,
            PageTableLevel::L3 => 256,
            _ => 512,
        }
    }
}

// Page Table Entry (PTE) for MMU v2 - 64-bit entry at level 4.
bitfield! {
    pub(crate) struct Pte(u64), "Page Table Entry for MMU v2" {
        0:0     valid               as bool, "Entry is valid";
        2:1     aperture            as u8 => AperturePte, "Memory aperture type";
        3:3     volatile            as bool, "Volatile (bypass L2 cache)";
        4:4     encrypted           as bool, "Encryption enabled (Confidential Computing)";
        5:5     privilege           as bool, "Privileged access only";
        6:6     read_only           as bool, "Write protection";
        7:7     atomic_disable      as bool, "Atomic operations disabled";
        53:8    frame_number_sys    as u64 => Pfn, "Frame number for system memory";
        32:8    frame_number_vid    as u64 => Pfn, "Frame number for video memory";
        35:33   peer_id             as u8, "Peer GPU ID for peer memory (0-7)";
        53:36   comptagline         as u32, "Compression tag line bits";
        63:56   kind                as u8, "Surface kind/format";
    }
}

impl PteOps for Pte {
    fn new(val: u64) -> Self {
        Self(val)
    }

    fn invalid() -> Self {
        Self::default()
    }

    fn new_vram(pfn: Pfn, writable: bool) -> Self {
        Self::default()
            .set_valid(true)
            .set_aperture(AperturePte::VideoMemory)
            .set_frame_number_vid(pfn)
            .set_read_only(!writable)
    }

    fn is_valid(&self) -> bool {
        self.valid()
    }

    fn frame_number(&self) -> Pfn {
        match self.aperture() {
            AperturePte::VideoMemory => self.frame_number_vid(),
            _ => self.frame_number_sys(),
        }
    }

    fn raw_u64(&self) -> u64 {
        self.0
    }
}

// Page Directory Entry (PDE) for MMU v2 - 64-bit entry at levels 0-2.
bitfield! {
    pub(crate) struct Pde(u64), "Page Directory Entry for MMU v2" {
        0:0     valid_inverted      as bool, "Valid bit (inverted logic)";
        2:1     aperture            as u8 => AperturePde, "Memory aperture type";
        3:3     volatile            as bool, "Volatile (bypass L2 cache)";
        5:5     no_ats              as bool, "Disable Address Translation Services";
        53:8    table_frame_sys     as u64 => Pfn, "Table frame number for system memory";
        32:8    table_frame_vid     as u64 => Pfn, "Table frame number for video memory";
        35:33   peer_id             as u8, "Peer GPU ID (0-7)";
    }
}

impl PdeOps for Pde {
    fn new(val: u64) -> Self {
        Self(val)
    }

    fn new_vram(table_pfn: Pfn) -> Self {
        Self::default()
            .set_valid_inverted(false) // 0 = valid
            .set_aperture(AperturePde::VideoMemory)
            .set_table_frame_vid(table_pfn)
    }

    fn is_valid(&self) -> bool {
        !self.valid_inverted() && (*self).aperture() != AperturePde::Invalid
    }

    fn aperture(&self) -> AperturePde {
        (*self).aperture()
    }

    fn table_frame(&self) -> Pfn {
        match self.aperture() {
            AperturePde::VideoMemory => self.table_frame_vid(),
            _ => self.table_frame_sys(),
        }
    }

    fn table_vram_address(&self) -> VramAddress {
        debug_assert!(
            (*self).aperture() == AperturePde::VideoMemory,
            "table_vram_address called on non-VRAM PDE (aperture: {:?})",
            (*self).aperture()
        );
        VramAddress::from(self.table_frame_vid())
    }

    fn raw_u64(&self) -> u64 {
        self.0
    }
}

/// Dual `PDE` at Level 3 - 128-bit entry of Large/Small Page Table pointers.
///
/// The dual `PDE` supports both large (64KB) and small (4KB) page tables.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DualPde {
    /// Large/Big Page Table pointer (lower 64 bits).
    pub(crate) big: Pde,
    /// Small Page Table pointer (upper 64 bits).
    pub(crate) small: Pde,
}

impl DualPdeOps for DualPde {
    fn new(big: u64, small: u64) -> Self {
        Self {
            big: Pde::new(big),
            small: Pde::new(small),
        }
    }

    // Note: The big (LPT) portion is set to 0, not `Pde::invalid()`.
    // According to hardware documentation, clearing bit 0 of the 128-bit
    // entry makes the PDE behave as a "normal" PDE. Using `Pde::invalid()`
    // would set bit 0 (valid_inverted), which breaks page table walking.
    fn new_small(table_pfn: Pfn) -> Self {
        Self {
            big: Pde::new(0),
            small: Pde::new_vram(table_pfn),
        }
    }

    fn has_small(&self) -> bool {
        self.small.is_valid()
    }

    fn has_big(&self) -> bool {
        self.big.is_valid()
    }

    fn small_pfn(&self) -> Pfn {
        self.small.table_frame()
    }

    fn small_vram_address(&self) -> VramAddress {
        self.small.table_vram_address()
    }

    fn big_raw_u64(&self) -> u64 {
        self.big.raw_u64()
    }

    fn small_raw_u64(&self) -> u64 {
        self.small.raw_u64()
    }
}

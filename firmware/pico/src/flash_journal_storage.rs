//! Access to the linker-reserved Pico calibration journal.

use core::{
    mem, ptr,
    sync::atomic::{Ordering, compiler_fence},
};

use crate::flash_xip::{
    RomFunction, hexaroam_erase_sector, hexaroam_program_slot,
};

use hexaroam_servo_storage::{
    JOURNAL_SIZE, JournalLayout, JournalRead, JournalStorage, LayoutError,
    SECTOR_SIZE, SLOT_SIZE,
};
use rp_pico::hal::rom_data;

const FLASH_START: usize = 0x1000_0000;
const FLASH_SIZE: u32 = 2 * 1024 * 1024;
const RAM_START: usize = 0x2000_0000;
const RAM_END: usize = 0x2004_0000;
const BOOT2_WORDS: usize = 64;

unsafe extern "C" {
    static __calibration_start: u8;
    static __calibration_end: u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashStorageError {
    InvalidLayout,
    UnknownSlot(usize),
    SourceNotInRam,
    SlotNotErased,
    InvalidExecutionAddress,
    VerificationFailed,
    UnknownSector(usize),
}

pub struct FlashJournalStorage {
    layout: JournalLayout,
}

fn ram_range_valid(address: usize, length: usize) -> bool {
    address >= RAM_START
        && address
            .checked_add(length)
            .is_some_and(|end| end <= RAM_END)
}

fn rom_address_valid(address: usize) -> bool {
    address & 1 != 0 && (address & !1_usize) < 0x4000
}

impl FlashJournalStorage {
    /// Creates access to the reserved calibration journal.
    ///
    /// # Safety
    ///
    /// The program must run on a Pico with readable 2-MiB XIP flash.
    /// The linker symbols must delimit the reserved journal region.
    ///
    /// For this object's entire lifetime, no independent flash writer
    /// or FlashJournalReader may coexist with it.
    /// XIP must remain enabled between backend operations.
    ///
    /// During future program/erase operations, the caller must ensure
    /// that no other core, DMA channel, debugger operation or unmasked
    /// exception accesses flash. The backend will mask normal interrupts.
    /// The standard Pico flash/pad configuration must be preserved.
    pub unsafe fn new() -> Result<Self, FlashStorageError> {
        let start = ptr::addr_of!(__calibration_start) as usize;
        let end = ptr::addr_of!(__calibration_end) as usize;

        if start < FLASH_START
            || end > FLASH_START + FLASH_SIZE as usize
            || end.checked_sub(start) != Some(JOURNAL_SIZE)
        {
            return Err(FlashStorageError::InvalidLayout);
        }

        let start_offset = (start - FLASH_START) as u32;
        let layout = JournalLayout::new(start_offset, FLASH_SIZE)
            .map_err(|_| FlashStorageError::InvalidLayout)?;

        Ok(Self { layout })
    }

    /// Programs and verifies one erased slot.
    ///
    /// Source data must reside entirely in RAM.
    /// This method never erases flash.
    pub(crate) fn program_slot(
        &mut self,
        slot_index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), FlashStorageError> {
        let range = self
            .layout
            .slot_range(slot_index)
            .map_err(|error| match error {
                LayoutError::UnknownSlot(index) => {
                    FlashStorageError::UnknownSlot(index)
                }
                _ => FlashStorageError::InvalidLayout,
            })?;

        if !ram_range_valid(data.as_ptr() as usize, data.len()) {
            return Err(FlashStorageError::SourceNotInRam);
        }

        let target = (FLASH_START + range.offset as usize) as *const u8;

        for index in 0..SLOT_SIZE {
            // SAFETY: XIP is active and JournalLayout bounds the whole slot.
            if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
                return Err(FlashStorageError::SlotNotErased);
            }
        }

        // Resolve ROM lookups before disabling XIP.
        let connect = rom_data::connect_internal_flash::ptr();
        let exit = rom_data::flash_exit_xip::ptr();
        let program = rom_data::flash_range_program::ptr();
        let flush = rom_data::flash_flush_cache::ptr();

        // Aligned, live RAM copy of the running image's Boot2.
        let mut boot2 = [0_u32; BOOT2_WORDS];
        for (index, word) in boot2.iter_mut().enumerate() {
            // SAFETY: XIP is active; reads stay within the 256-byte Boot2.
            *word = unsafe {
                ptr::read_volatile((FLASH_START as *const u32).add(index))
            };
        }

        let boot2_address = boot2.as_ptr() as usize;
        let routine_address =
            (hexaroam_program_slot as *const () as usize) & !1_usize;

        let rom_addresses = [
            connect as *const () as usize,
            exit as *const () as usize,
            program as *const () as usize,
            flush as *const () as usize,
        ];

        if !ram_range_valid(routine_address, 1)
            || !ram_range_valid(boot2_address, size_of_val(&boot2))
            || !rom_addresses
                .into_iter()
                .all(rom_address_valid)
        {
            return Err(FlashStorageError::InvalidExecutionAddress);
        }

        // SAFETY: The aligned Boot2 copy remains live in RAM during the call.
        // Set the Cortex-M Thumb-state bit.
        let restore =
            unsafe { mem::transmute::<usize, RomFunction>(boot2_address | 1) };

        compiler_fence(Ordering::SeqCst);

        cortex_m::interrupt::free(|_| {
            // SAFETY:
            // The constructor's contract excludes competing flash activity
            // and flash access by other cores, DMA or unmasked exceptions.
            // The target is a bounded, erased journal slot.
            // Source data and Boot2 are in RAM and remain live.
            // ROM pointers were resolved and checked while XIP was active.
            // The RAM routine restores XIP before returning.
            unsafe {
                hexaroam_program_slot(
                    connect,
                    exit,
                    program,
                    flush,
                    restore,
                    range.offset,
                    data.as_ptr(),
                );
            }
        });

        compiler_fence(Ordering::SeqCst);

        for (index, expected) in data.iter().enumerate() {
            // SAFETY: XIP is restored; target remains inside the checked slot.
            let actual = unsafe { ptr::read_volatile(target.add(index)) };

            if actual != *expected {
                return Err(FlashStorageError::VerificationFailed);
            }
        }

        Ok(())
    }

    /// Erases and verifies one journal sector.
    ///
    /// Already erased sectors succeed without another erase operation.
    /// The caller selects the expendable sector.
    fn erase_journal_sector(
        &mut self,
        sector_index: usize,
    ) -> Result<(), FlashStorageError> {
        let range = self
            .layout
            .sector_range(sector_index)
            .map_err(|error| match error {
                LayoutError::UnknownSector(index) => {
                    FlashStorageError::UnknownSector(index)
                }
                _ => FlashStorageError::InvalidLayout,
            })?;

        let target = (FLASH_START + range.offset as usize) as *const u8;

        let mut occupied = false;
        for index in 0..SECTOR_SIZE {
            // SAFETY: XIP is active; JournalLayout bounds the whole sector.
            if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
                occupied = true;
                break;
            }
        }

        if !occupied {
            return Ok(());
        }

        // Resolve ROM lookups before disabling XIP.
        let connect = rom_data::connect_internal_flash::ptr();
        let exit = rom_data::flash_exit_xip::ptr();
        let erase = rom_data::flash_range_erase::ptr();
        let flush = rom_data::flash_flush_cache::ptr();

        let mut boot2 = [0_u32; BOOT2_WORDS];
        for (index, word) in boot2.iter_mut().enumerate() {
            // SAFETY: XIP is active; reads stay within the 256-byte Boot2.
            *word = unsafe {
                ptr::read_volatile((FLASH_START as *const u32).add(index))
            };
        }

        let boot2_address = boot2.as_ptr() as usize;
        let routine_address =
            (hexaroam_erase_sector as *const () as usize) & !1_usize;

        let rom_addresses = [
            connect as *const () as usize,
            exit as *const () as usize,
            erase as *const () as usize,
            flush as *const () as usize,
        ];

        if !ram_range_valid(routine_address, 1)
            || !ram_range_valid(boot2_address, size_of_val(&boot2))
            || !rom_addresses
                .into_iter()
                .all(rom_address_valid)
        {
            return Err(FlashStorageError::InvalidExecutionAddress);
        }

        // SAFETY: The aligned Boot2 copy remains live in RAM during the call.
        // Set the Cortex-M Thumb-state bit.
        let restore =
            unsafe { mem::transmute::<usize, RomFunction>(boot2_address | 1) };

        compiler_fence(Ordering::SeqCst);

        cortex_m::interrupt::free(|_| {
            // SAFETY:
            // The constructor's contract excludes competing flash activity
            // and flash access by other cores, DMA or unmasked exceptions.
            // JournalLayout bounds the selected sector in the reserved region.
            // The caller selects the sector whose contents may be discarded.
            // Boot2 remains live in RAM; ROM pointers were checked above.
            // The RAM routine restores XIP before returning.
            unsafe {
                hexaroam_erase_sector(
                    connect,
                    exit,
                    erase,
                    flush,
                    restore,
                    range.offset,
                );
            }
        });

        compiler_fence(Ordering::SeqCst);

        for index in 0..SECTOR_SIZE {
            // SAFETY: XIP is restored; the whole sector remains bounded.
            if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
                return Err(FlashStorageError::VerificationFailed);
            }
        }

        Ok(())
    }
}

impl JournalRead for FlashJournalStorage {
    type Error = FlashStorageError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        let range = self
            .layout
            .slot_range(slot_index)
            .map_err(|error| match error {
                LayoutError::UnknownSlot(index) => {
                    FlashStorageError::UnknownSlot(index)
                }
                _ => FlashStorageError::InvalidLayout,
            })?;

        let address = FLASH_START + range.offset as usize;

        // SAFETY:
        // Construction validates the reserved readable XIP region.
        // JournalLayout bounds the entire source slot.
        // The caller provides a separate writable destination.
        // No operation disables XIP during this copy.
        unsafe {
            ptr::copy_nonoverlapping(
                address as *const u8,
                output.as_mut_ptr(),
                range.length,
            );
        }

        Ok(())
    }
}

impl JournalStorage for FlashJournalStorage {
    fn write_slot(
        &mut self,
        slot_index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.program_slot(slot_index, data)
    }

    fn erase_sector(&mut self, sector_index: usize) -> Result<(), Self::Error> {
        self.erase_journal_sector(sector_index)
    }
}

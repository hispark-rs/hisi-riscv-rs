//! Derive the four SDK mailboxes from a copied Ccore boot header.
//!
//! The SDK reads `ccore_cc_symbols_t.ipc_share_mem_start` (word6) from Ccore
//! local0x10000, then uses RX0/RX1/TX0/TX1 extents0x500/0x100/0x500/0x100.
//! Ccore-local and application-view addresses are distinct types. Supply an
//! explicitly qualified Ccore SRAM range: parsing checks representation and
//! containment, not power, MPU/cache attributes, firmware compatibility or a
//! peer lease. A successful descriptor never grants a live shared-memory token.
//! No memory is accessed, initialized or modified by this module.

use super::Channel;
use super::mailbox::{MailboxSize, Route};

const ALIAS_OFFSET: u32 = 0x1000_0000;
const CONTROL_START: u32 = 0x10000;
// Largest SDK CT_SRAM_END, for device/test configurations. This is an upper
// representability bound; it does not select a mode or claim all of it exists.
const SDK_MAX_SRAM_END: u32 = 0x23f00;
const BOOT_HEADER_END: u32 = CONTROL_START + 0x80;
const SHARED_BYTES: u32 = 0xc00;

/// A bounded Ccore-local address derived from an explicit SRAM range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ControlLocalAddress(u32);
impl ControlLocalAddress {
    /// Numeric local address; not an application pointer or ownership proof.
    pub const fn raw(self) -> u32 {
        self.0
    }
    /// SDK ACCORE_MEM_ADDR_OFFSET alias. Does not establish mapping/coherency.
    pub const fn application_view(self) -> ApplicationViewAddress {
        // Containment is checked below SDK_MAX_SRAM_END, so this cannot wrap.
        ApplicationViewAddress(self.0 + ALIAS_OFFSET)
    }
}

/// An application-core view, distinct from a control-core local address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ApplicationViewAddress(u32);
impl ApplicationViewAddress {
    /// Numeric address only; dereferencing still requires platform ownership.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Explicit available Ccore SRAM `[start,end)`, excluding reserved memory.
/// The caller supplies the actual mode/board range; there is no default mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ControlSramRange {
    start: u32,
    end: u32,
}
impl ControlSramRange {
    /// Reject reversed, unaligned or out-of-SDK bounds. This validates numbers,
    /// not whether a memory bank is present or owned on the running board.
    pub const fn try_from_bounds(start: u32, end: u32) -> Option<Self> {
        if start < CONTROL_START
            || end > SDK_MAX_SRAM_END
            || start >= end
            || !start.is_multiple_of(4)
            || !end.is_multiple_of(4)
        {
            None
        } else {
            Some(Self { start, end })
        }
    }
}

/// A rejected placement. No live memory is touched on either success or error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LayoutError {
    /// Shared region base is not four-byte aligned.
    Alignment,
    /// Extent wraps or lies outside the explicitly supplied SRAM range.
    OutsideSram,
    /// Shared memory would overlap the fixed128-byte control startup header.
    BootHeaderOverlap,
}

/// One mailbox's route, local base and validated total extent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MailboxPlacement {
    route: Route,
    base: ControlLocalAddress,
    size: MailboxSize,
}
impl MailboxPlacement {
    /// Direction/channel, as seen by the application core.
    pub const fn route(self) -> Route {
        self.route
    }
    /// Control-core local base, not a Rust reference.
    pub const fn control_address(self) -> ControlLocalAddress {
        self.base
    }
    /// Twelve-byte metadata plus data capacity.
    pub const fn size(self) -> MailboxSize {
        self.size
    }
}

/// SDK RX0/RX1/TX0/TX1 placements from a copied seven-word boot header.
/// Does not inspect queues, start a core, negotiate a service or attach a role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SdkMailboxLayout {
    placements: [MailboxPlacement; 4],
}
impl SdkMailboxLayout {
    /// Parse only the IPC pointer at bytes24..28. The two reserved words and
    /// startup/log fields are opaque; they do not identify a firmware version.
    /// `sram` must describe the separately qualified application-accessible bank.
    pub fn from_boot_header(
        header: &[u8; 28],
        sram: ControlSramRange,
    ) -> Result<Self, LayoutError> {
        let base = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        if !base.is_multiple_of(4) {
            return Err(LayoutError::Alignment);
        }
        let end = base
            .checked_add(SHARED_BYTES)
            .ok_or(LayoutError::OutsideSram)?;
        if base < sram.start || end > sram.end {
            return Err(LayoutError::OutsideSram);
        }
        if base < BOOT_HEADER_END {
            return Err(LayoutError::BootHeaderOverlap);
        }
        let large = MailboxSize::try_from_total_bytes(0x500).ok_or(LayoutError::OutsideSram)?;
        let small = MailboxSize::try_from_total_bytes(0x100).ok_or(LayoutError::OutsideSram)?;
        Ok(Self {
            placements: [
                MailboxPlacement {
                    route: Route::ControlToApplication(Channel::Channel0),
                    base: ControlLocalAddress(base),
                    size: large,
                },
                MailboxPlacement {
                    route: Route::ControlToApplication(Channel::Channel1),
                    base: ControlLocalAddress(base + 0x500),
                    size: small,
                },
                MailboxPlacement {
                    route: Route::ApplicationToControl(Channel::Channel0),
                    base: ControlLocalAddress(base + 0x600),
                    size: large,
                },
                MailboxPlacement {
                    route: Route::ApplicationToControl(Channel::Channel1),
                    base: ControlLocalAddress(base + 0xb00),
                    size: small,
                },
            ],
        })
    }

    /// All placements in the SDK's allocation order: RX0/RX1/TX0/TX1.
    pub const fn placements(&self) -> &[MailboxPlacement; 4] {
        &self.placements
    }

    /// Select by typed route/channel, without a numeric core ID or array index.
    pub const fn for_route(&self, route: Route) -> &MailboxPlacement {
        &self.placements[match route {
            Route::ControlToApplication(Channel::Channel0) => 0,
            Route::ControlToApplication(Channel::Channel1) => 1,
            Route::ApplicationToControl(Channel::Channel0) => 2,
            Route::ApplicationToControl(Channel::Channel1) => 3,
        }]
    }
}

#[cfg(all(test, not(target_arch = "riscv32")))]
#[path = "layout_tests.rs"]
mod tests;

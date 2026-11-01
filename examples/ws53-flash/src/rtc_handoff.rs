//! Only the two application RTCs; no common RTC/clock-bank writes.
//! Reject inherited clients rather than stopping a running alarm for testing.
pub fn idle_profile() -> bool {
    macro_rules! idle {
        ($pac:ty, $irq:expr) => {{
            let r = unsafe { &*<$pac>::PTR };
            r.control().read().bits() == 0
                && r.load_count1().read().bits() == 0
                && r.raw_intr().read().bits() == 0
                && !hisi_hal::interrupt::is_enabled($irq)
                && !hisi_hal::interrupt::is_pending($irq)
        }};
    }
    idle!(ws53_pac::Rtc0, hisi_hal::interrupt::Interrupt::RTC_0)
        && idle!(ws53_pac::Rtc1, hisi_hal::interrupt::Interrupt::RTC_1)
}

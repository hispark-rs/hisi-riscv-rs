#![no_std]
pub fn timer_irq() -> u16 { ws53_pac::interrupt::ExternalInterrupt::TIMER_1 as u16 }
pub fn take() -> Option<ws53_pac::Peripherals> { ws53_pac::Peripherals::take() }
#[cfg(test)]
mod tests {
    #[test]
    fn external_registry_singleton_and_irq() {
        assert_eq!(super::timer_irq(), 54);
        assert!(super::take().is_some());
        assert!(super::take().is_none());
    }
}

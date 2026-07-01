# Unsafe Verification Readiness

- Date: 2026-07-01
- Scope: crates/hisi-riscv-hal/src
- Unsafe occurrences: 486

## Unsafe Occurrences By File

- crates/hisi-riscv-hal/src/pwm.rs: 86
- crates/hisi-riscv-hal/src/io_config.rs: 45
- crates/hisi-riscv-hal/src/dma.rs: 36
- crates/hisi-riscv-hal/src/gpio.rs: 31
- crates/hisi-riscv-hal/src/spi.rs: 26
- crates/hisi-riscv-hal/src/interrupt.rs: 25
- crates/hisi-riscv-hal/src/timer.rs: 22
- crates/hisi-riscv-hal/src/sfc.rs: 19
- crates/hisi-riscv-hal/src/uart.rs: 18
- crates/hisi-riscv-hal/src/i2s.rs: 17
- crates/hisi-riscv-hal/src/i2c.rs: 17
- crates/hisi-riscv-hal/src/tsensor.rs: 15
- crates/hisi-riscv-hal/src/gadc.rs: 13
- crates/hisi-riscv-hal/src/ulp_gpio.rs: 12
- crates/hisi-riscv-hal/src/wdt.rs: 11
- crates/hisi-riscv-hal/src/cache.rs: 10
- crates/hisi-riscv-hal/src/lsadc.rs: 9
- crates/hisi-riscv-hal/src/rtc.rs: 8
- crates/hisi-riscv-hal/src/peripherals.rs: 7
- crates/hisi-riscv-hal/src/clock_init.rs: 7
- crates/hisi-riscv-hal/src/embassy.rs: 6
- crates/hisi-riscv-hal/src/tcxo.rs: 5
- crates/hisi-riscv-hal/src/efuse.rs: 5
- crates/hisi-riscv-hal/src/pdm.rs: 4
- crates/hisi-riscv-hal/src/km.rs: 4
- crates/hisi-riscv-hal/src/i2c_v151.rs: 4
- crates/hisi-riscv-hal/src/usb.rs: 3
- crates/hisi-riscv-hal/src/trng.rs: 3
- crates/hisi-riscv-hal/src/system.rs: 3
- crates/hisi-riscv-hal/src/time.rs: 2
- crates/hisi-riscv-hal/src/soc/ws63.rs: 2
- crates/hisi-riscv-hal/src/rtc_v150.rs: 2
- crates/hisi-riscv-hal/src/keyscan.rs: 2
- crates/hisi-riscv-hal/src/asynch.rs: 2
- crates/hisi-riscv-hal/src/trng_v1.rs: 1
- crates/hisi-riscv-hal/src/spacc.rs: 1
- crates/hisi-riscv-hal/src/safety.rs: 1
- crates/hisi-riscv-hal/src/qdec.rs: 1
- crates/hisi-riscv-hal/src/pke.rs: 1

## Safe To Unsafe Forwarding Candidates

Heuristic only. Review manually; this intentionally does not decide soundness.

- crates/hisi-riscv-hal/src/gpio.rs:113: number wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:133: init_input wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:140: init_output wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:152: init_flex wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:174: is_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:179: is_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:184: number wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:189: enable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:195: disable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:201: clear_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:209: set_interrupt_trigger wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:223: interrupt_pending wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:229: degrade wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:257: set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:262: set_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:267: toggle wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:278: is_set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:283: number wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:290: into_flex wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:303: degrade wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:372: set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:378: set_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:384: toggle wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:396: is_set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:402: is_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:419: is_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:437: number wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:443: degrade wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:582: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:586: register_block wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gpio.rs:616: on_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/keyscan.rs:41: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:31: number wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:38: set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:43: set_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:48: toggle wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:58: is_set_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:63: into_input wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:71: is_high wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:76: is_low wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:81: enable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:86: disable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:91: clear_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:96: interrupt_pending wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:101: into_output wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:143: create_input_pin wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/ulp_gpio.rs:151: create_output_pin wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/embassy.rs:133: on_alarm_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tcxo.rs:34: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tcxo.rs:44: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tcxo.rs:52: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tcxo.rs:60: clear wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gadc.rs:151: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/gadc.rs:206: read wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc_v150.rs:54: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:109: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:121: configure_global wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:148: configure_timing wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:168: configure_bus wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:187: release_bus_reset wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:194: hold_bus_reset wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:287: send_command wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:327: command_with_data wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:405: bus_dma_start wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:426: bus_dma_wait wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:432: dma_done wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:437: command_done wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:442: clear_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:452: enable_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:468: raw_interrupt_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:476: enable_aes_low_power wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:483: disable_aes_low_power wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/sfc.rs:490: set_iv_valid wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spacc.rs:70: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:162: new_spi1 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:249: transfer wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:266: write wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:347: write_dma wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:476: transfer_dma wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/spi.rs:629: release wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:144: new_uart0 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:152: new_uart1 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:160: new_uart2 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:226: write_byte wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:256: write wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:346: write_dma wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:409: read_dma wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/uart.rs:631: on_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:76: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:87: set_clock_period wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:94: status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:109: read_byte wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:121: read_buffer wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/efuse.rs:135: write_byte wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/asynch.rs:50: block_on wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/km.rs:20: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/km.rs:30: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/km.rs:42: is_keyslot_locked wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/km.rs:61: lock_keyslot wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/peripherals.rs:31: ptr wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/peripherals.rs:41: register_block wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/peripherals.rs:53: reborrow wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/peripherals.rs:75: take wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:137: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:179: configure wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:220: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:235: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:247: feed wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:261: counter_value wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:284: interrupt_pending wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:289: interrupt_masked wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:294: clear_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:299: enable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/wdt.rs:309: disable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c_v151.rs:92: new_i2c0 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c_v151.rs:99: new_i2c1 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c_v151.rs:181: probe wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c_v151.rs:204: write wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pke.rs:30: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:34: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:44: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:53: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:65: set_mode wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:77: start_conversion wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:84: data_ready wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:91: read_raw wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:100: read_blocking wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:107: clear_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:118: set_high_limit wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:127: set_low_limit wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:134: set_over_temp_threshold wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:149: enable_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:166: disable_all_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:175: interrupt_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:181: clear_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:201: configure_auto_refresh wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:209: enable_calibration wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/tsensor.rs:217: disable_calibration wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:160: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:193: configure wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:254: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:268: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:283: set_polarity wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:299: start wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:304: set_pulse_count wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pwm.rs:324: into_running wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/time.rs:15: now wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:267: new_master wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:292: new_slave wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:332: enable_tx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:339: enable_rx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:346: disable_tx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:353: disable_rx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:360: reset_tx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:367: reset_rx wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:374: enable_tx_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:381: enable_rx_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:388: write_left wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:395: write_right wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:402: read_left wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:407: read_right wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:412: tx_fifo_left_depth wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:417: tx_fifo_right_depth wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:422: rx_fifo_left_depth wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:427: rx_fifo_right_depth wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:434: interrupt_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:440: clear_interrupts wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:452: set_interrupt_mask wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2s.rs:478: version wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/usb.rs:74: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/usb.rs:80: core_id wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/usb.rs:85: is_present wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/usb.rs:96: device_enumerate wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:70: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:90: configure wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:103: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:111: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:119: set_load wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:126: current_value wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:131: enable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/rtc.rs:139: disable_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/soc/ws63.rs:90: uart_boot_clock_hz wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/clock_init.rs:103: detect wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/clock_init.rs:197: init_clocks wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/trng.rs:34: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/trng.rs:83: fill_bytes wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/trng.rs:101: fill_words wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/trng.rs:111: set_sample_clock wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/trng.rs:121: set_divider wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:74: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:89: configure wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:107: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:124: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:150: current_value wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:305: current wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:389: current wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:494: on_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/timer.rs:550: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pdm.rs:36: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pdm.rs:66: version wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/pdm.rs:74: capture wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:179: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:189: enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:194: disable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:202: set_analog_enable wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:211: configure_scan wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:226: start_scan wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:231: stop_scan wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:238: set_fifo_waterline wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:245: data_ready wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:253: read_sample wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:261: enable_cic_filter wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:270: disable_cic_filter wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:275: set_offset wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:280: set_gain wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/lsadc.rs:400: on_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:124: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:139: set_gpio_mux wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:194: gpio_mux wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:218: set_uart_mux wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:243: configure_gpio_pad wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:305: configure_uart_pad wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/io_config.rs:333: configure_sfc_pad wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:310: is_enabled wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:328: set_priority wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:338: priority wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:354: set_threshold wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:359: threshold wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:367: clear_pending wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:380: is_pending wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:399: init wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:433: disable_global wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/interrupt.rs:442: free wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:187: new wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:210: enable_controller wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:226: disable_controller wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:239: configure_channel wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:329: enable_channel wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:339: disable_channel wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:349: channel_enabled wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:356: channel_active wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:362: halt_channel wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:372: resume_channel wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:382: burst_request wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:390: single_request wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:403: raw_interrupt_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:413: interrupt_status wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:419: clear_transfer_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:427: clear_error_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:438: set_sync wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:600: start_mem_to_mem wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:645: is_done wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:659: wait wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:949: start_mem_to_peripheral wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:997: start_peripheral_to_mem wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:1061: is_done wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:1075: wait wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/dma.rs:1616: on_interrupt wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c.rs:53: new_i2c0 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c.rs:61: new_i2c1 wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c.rs:162: write wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c.rs:187: read wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/i2c.rs:219: write_read wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/system.rs:63: reset_reason wraps or reaches unsafe within the next 40 lines
- crates/hisi-riscv-hal/src/system.rs:85: software_reset wraps or reaches unsafe within the next 40 lines

## Clippy undocumented_unsafe_blocks Baseline

```text
157 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:192:9
    |
192 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:208:9
    |
208 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:223:9
    |
223 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:238:9
    |
238 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:249:9
    |
249 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:271:9
    |
271 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:302:9
    |
302 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: unsafe block missing a safety comment
   --> crates/hisi-riscv-hal/src/wdt.rs:312:9
    |
312 |         unsafe {
    |         ^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.96.0/index.html#undocumented_unsafe_blocks

warning: `hisi-riscv-hal` (lib) generated 390 warnings
    Finished `dev` profile [optimized + debuginfo] target(s) in 1.75s
```

- Clippy exit status: 0
- Undocumented unsafe warnings in captured output: 390

## Miri Readiness

- Miri is not installed or nightly is unavailable. No Miri gate ran.

## Kani Readiness

- Kani tooling not found. No model-checking gate ran.

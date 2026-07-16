//! Native hostap 2.11 SAE bignum and group 19 ABI.

#[cfg(target_arch = "riscv32")]
use core::{
    cmp::Ordering,
    ffi::{c_int, c_uint},
    mem::{align_of, size_of},
    ptr,
};

#[cfg(target_arch = "riscv32")]
use hisi_crypto::sae::{
    BIGNUM_BYTES, BignumArithmetic, BignumEncoding, BignumRandom, GROUP_19, Group19,
    LegendreSymbol, RustCryptoBignum, RustCryptoGroup19,
};
#[cfg(all(test, not(target_arch = "riscv32")))]
use hisi_crypto::sae::{GROUP_19, Group19, RustCryptoGroup19};
#[cfg(target_arch = "riscv32")]
use hisi_crypto::{
    CryptoError,
    sae::{P256_ELEMENT_BYTES, SaeBignum, SaeP256Point},
};

#[cfg(target_arch = "riscv32")]
const BIGNUM_MAGIC: u32 = 0x424e_3634;
#[cfg(target_arch = "riscv32")]
const EC_MAGIC: u32 = 0x4543_3139;
#[cfg(target_arch = "riscv32")]
const POINT_MAGIC: u32 = 0x5054_3139;
#[cfg(target_arch = "riscv32")]
const OWNED: u32 = 1;
#[cfg(target_arch = "riscv32")]
const BORROWED: u32 = 0;
#[cfg(target_arch = "riscv32")]
const MAX_RANDOM_REJECTIONS: usize = 128;

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct BignumObject {
    magic: u32,
    ownership: u32,
    value: SaeBignum,
}

#[cfg(target_arch = "riscv32")]
impl BignumObject {
    fn owned(value: SaeBignum) -> Self {
        Self {
            magic: BIGNUM_MAGIC,
            ownership: OWNED,
            value,
        }
    }

    fn borrowed(value: SaeBignum) -> Self {
        Self {
            magic: BIGNUM_MAGIC,
            ownership: BORROWED,
            value,
        }
    }
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct EcObject {
    magic: u32,
    group: RustCryptoGroup19,
    prime: BignumObject,
    order: BignumObject,
    coefficient_a: BignumObject,
    coefficient_b: BignumObject,
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct PointObject {
    magic: u32,
    value: SaeP256Point,
}

#[cfg(any(target_arch = "riscv32", test))]
fn checked_range(address: usize, length: usize) -> Option<(usize, usize)> {
    address.checked_add(length).map(|end| (address, end))
}

#[cfg(any(target_arch = "riscv32", test))]
fn ranges_overlap(a: usize, a_len: usize, b: usize, b_len: usize) -> Option<bool> {
    let (a_start, a_end) = checked_range(a, a_len)?;
    let (b_start, b_end) = checked_range(b, b_len)?;
    Some(a_start < b_end && b_start < a_end)
}

#[cfg(any(target_arch = "riscv32", test))]
fn object_alias_is_valid(a: usize, a_len: usize, b: usize, b_len: usize) -> bool {
    a == b || matches!(ranges_overlap(a, a_len, b, b_len), Some(false))
}

#[cfg(any(target_arch = "riscv32", test))]
fn entropy_top_mask(first_modulus_byte: u8) -> Option<u8> {
    (first_modulus_byte != 0).then(|| u8::MAX >> first_modulus_byte.leading_zeros())
}

#[cfg(target_arch = "riscv32")]
fn clear_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { ptr::write_volatile(byte, 0) };
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[cfg(target_arch = "riscv32")]
fn aligned<T>(pointer: *const T) -> bool {
    !pointer.is_null() && pointer.addr().is_multiple_of(align_of::<T>())
}

#[cfg(target_arch = "riscv32")]
unsafe fn allocate<T>(value: T) -> *mut T {
    let allocation = unsafe { super::os_zalloc(size_of::<T>()) }.cast::<T>();
    if !aligned(allocation) {
        if !allocation.is_null() {
            unsafe { super::os_free(allocation.cast()) };
        }
        return ptr::null_mut();
    }
    unsafe { allocation.write(value) };
    allocation
}

#[cfg(target_arch = "riscv32")]
unsafe fn deallocate<T>(pointer: *mut T) {
    unsafe { ptr::drop_in_place(pointer) };
    unsafe { super::os_free(pointer.cast()) };
}

#[cfg(target_arch = "riscv32")]
unsafe fn bignum_clone(pointer: *const BignumObject) -> Option<SaeBignum> {
    if !aligned(pointer) {
        return None;
    }
    let object = unsafe { &*pointer };
    (object.magic == BIGNUM_MAGIC).then(|| object.value.clone())
}

#[cfg(target_arch = "riscv32")]
unsafe fn bignum_is_owned(pointer: *const BignumObject) -> bool {
    if !aligned(pointer) {
        return false;
    }
    let object = unsafe { &*pointer };
    object.magic == BIGNUM_MAGIC && object.ownership == OWNED
}

#[cfg(target_arch = "riscv32")]
unsafe fn bignum_store(pointer: *mut BignumObject, value: SaeBignum) -> Result<(), ()> {
    if !aligned(pointer) {
        return Err(());
    }
    let object = unsafe { &mut *pointer };
    if object.magic != BIGNUM_MAGIC || object.ownership != OWNED {
        return Err(());
    }
    object.value = value;
    Ok(())
}

#[cfg(target_arch = "riscv32")]
unsafe fn ec_group(pointer: *const EcObject) -> Option<RustCryptoGroup19> {
    if !aligned(pointer) {
        return None;
    }
    let object = unsafe { &*pointer };
    (object.magic == EC_MAGIC && object.group.group_id() == GROUP_19).then_some(object.group)
}

#[cfg(target_arch = "riscv32")]
unsafe fn point_clone(pointer: *const PointObject) -> Option<SaeP256Point> {
    if !aligned(pointer) {
        return None;
    }
    let object = unsafe { &*pointer };
    (object.magic == POINT_MAGIC).then(|| object.value.clone())
}

#[cfg(target_arch = "riscv32")]
unsafe fn point_store(pointer: *mut PointObject, value: SaeP256Point) -> Result<(), ()> {
    if !aligned(pointer) {
        return Err(());
    }
    let object = unsafe { &mut *pointer };
    if object.magic != POINT_MAGIC {
        return Err(());
    }
    object.value = value;
    Ok(())
}

#[cfg(target_arch = "riscv32")]
fn bignum_aliases<const N: usize>(
    output: *mut BignumObject,
    inputs: [*const BignumObject; N],
) -> bool {
    if output.is_null() {
        return false;
    }
    for (index, input) in inputs.iter().enumerate() {
        if input.is_null()
            || !object_alias_is_valid(
                output.addr(),
                size_of::<BignumObject>(),
                input.addr(),
                size_of::<BignumObject>(),
            )
        {
            return false;
        }
        for other in &inputs[..index] {
            if !object_alias_is_valid(
                input.addr(),
                size_of::<BignumObject>(),
                other.addr(),
                size_of::<BignumObject>(),
            ) {
                return false;
            }
        }
    }
    true
}

#[cfg(target_arch = "riscv32")]
unsafe fn bignum_binary(
    a: *const BignumObject,
    b: *const BignumObject,
    result: *mut BignumObject,
    operation: impl FnOnce(&SaeBignum, &SaeBignum) -> Result<SaeBignum, CryptoError>,
) -> c_int {
    if !bignum_aliases(result, [a, b]) {
        return -1;
    }
    let (Some(a), Some(b)) = (unsafe { bignum_clone(a) }, unsafe { bignum_clone(b) }) else {
        return -1;
    };
    let Ok(value) = operation(&a, &b) else {
        return -1;
    };
    unsafe { bignum_store(result, value) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
unsafe fn bignum_ternary(
    a: *const BignumObject,
    b: *const BignumObject,
    modulus: *const BignumObject,
    result: *mut BignumObject,
    operation: impl FnOnce(&SaeBignum, &SaeBignum, &SaeBignum) -> Result<SaeBignum, CryptoError>,
) -> c_int {
    if !bignum_aliases(result, [a, b, modulus]) {
        return -1;
    }
    let (Some(a), Some(b), Some(modulus)) = (
        unsafe { bignum_clone(a) },
        unsafe { bignum_clone(b) },
        unsafe { bignum_clone(modulus) },
    ) else {
        return -1;
    };
    let Ok(value) = operation(&a, &b, &modulus) else {
        return -1;
    };
    unsafe { bignum_store(result, value) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_init() -> *mut BignumObject {
    unsafe { allocate(BignumObject::owned(RustCryptoBignum.init())) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_init_set(buffer: *const u8, length: usize) -> *mut BignumObject {
    if length > BIGNUM_BYTES || (length != 0 && buffer.is_null()) {
        return ptr::null_mut();
    }
    let bytes = if length == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(buffer, length) }
    };
    let Ok(value) = RustCryptoBignum.init_set(bytes) else {
        return ptr::null_mut();
    };
    unsafe { allocate(BignumObject::owned(value)) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_init_uint(value: c_uint) -> *mut BignumObject {
    unsafe { allocate(BignumObject::owned(RustCryptoBignum.init_u32(value))) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_deinit(pointer: *mut BignumObject, clear: c_int) {
    let _ = clear;
    if !unsafe { bignum_is_owned(pointer) } {
        return;
    }
    // SaeBignum zeroizes on every drop, which is stronger than hostap's clear flag.
    unsafe { deallocate(pointer) };
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_to_bin(
    value: *const BignumObject,
    output: *mut u8,
    output_len: usize,
    pad_to: usize,
) -> c_int {
    let write_capacity = if pad_to == 0 {
        output_len.min(BIGNUM_BYTES)
    } else {
        pad_to
    };
    if output.is_null()
        || pad_to > output_len
        || pad_to > BIGNUM_BYTES
        || !matches!(
            ranges_overlap(
                output.addr(),
                write_capacity,
                value.addr(),
                size_of::<BignumObject>(),
            ),
            Some(false)
        )
    {
        return -1;
    }
    let Some(value) = (unsafe { bignum_clone(value) }) else {
        return -1;
    };
    let output = unsafe { core::slice::from_raw_parts_mut(output, write_capacity) };
    RustCryptoBignum
        .write_be(&value, output, pad_to)
        .ok()
        .and_then(|written| c_int::try_from(written).ok())
        .unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_rand(
    result: *mut BignumObject,
    modulus: *const BignumObject,
) -> c_int {
    if !bignum_aliases(result, [modulus]) {
        return -1;
    }
    let Some(modulus) = (unsafe { bignum_clone(modulus) }) else {
        return -1;
    };
    let mut modulus_bytes = [0u8; BIGNUM_BYTES];
    let Ok(width) = RustCryptoBignum.write_be(&modulus, &mut modulus_bytes, 0) else {
        return -1;
    };
    let Some(top_mask) = modulus_bytes.first().copied().and_then(entropy_top_mask) else {
        return -1;
    };
    let mut entropy = [0u8; BIGNUM_BYTES];
    for _ in 0..MAX_RANDOM_REJECTIONS {
        if super::fill_hardware_entropy(&mut entropy[..width]).is_err() {
            clear_bytes(&mut entropy);
            return -1;
        }
        entropy[0] &= top_mask;
        match RustCryptoBignum.random_below(&entropy[..width], &modulus) {
            Ok(value) => {
                clear_bytes(&mut entropy);
                return unsafe { bignum_store(result, value) }.map_or(-1, |()| 0);
            }
            Err(CryptoError::EntropyRejected) => clear_bytes(&mut entropy[..width]),
            Err(_) => {
                clear_bytes(&mut entropy);
                return -1;
            }
        }
    }
    clear_bytes(&mut entropy);
    -1
}

macro_rules! bignum_binary_abi {
    ($name:ident, $method:ident) => {
        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name(
            a: *const BignumObject,
            b: *const BignumObject,
            result: *mut BignumObject,
        ) -> c_int {
            unsafe { bignum_binary(a, b, result, |a, b| RustCryptoBignum.$method(a, b)) }
        }
    };
}

bignum_binary_abi!(crypto_bignum_add, add);
bignum_binary_abi!(crypto_bignum_sub, sub);
bignum_binary_abi!(crypto_bignum_div, div);
bignum_binary_abi!(crypto_bignum_mod, modulo);
bignum_binary_abi!(crypto_bignum_inverse, inverse);
bignum_binary_abi!(crypto_bignum_sqrmod, square_mod);

macro_rules! bignum_ternary_abi {
    ($name:ident, $method:ident) => {
        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name(
            a: *const BignumObject,
            b: *const BignumObject,
            modulus: *const BignumObject,
            result: *mut BignumObject,
        ) -> c_int {
            unsafe {
                bignum_ternary(a, b, modulus, result, |a, b, modulus| {
                    RustCryptoBignum.$method(a, b, modulus)
                })
            }
        }
    };
}

bignum_ternary_abi!(crypto_bignum_addmod, add_mod);
bignum_ternary_abi!(crypto_bignum_mulmod, mul_mod);
bignum_ternary_abi!(crypto_bignum_exptmod, exp_mod);

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_rshift(
    value: *const BignumObject,
    bits: c_int,
    result: *mut BignumObject,
) -> c_int {
    if bits < 0 || !bignum_aliases(result, [value]) {
        return -1;
    }
    let Some(value) = (unsafe { bignum_clone(value) }) else {
        return -1;
    };
    let shifted = RustCryptoBignum.rshift(&value, bits as u32);
    unsafe { bignum_store(result, shifted) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_cmp(a: *const BignumObject, b: *const BignumObject) -> c_int {
    if !object_alias_is_valid(
        a.addr(),
        size_of::<BignumObject>(),
        b.addr(),
        size_of::<BignumObject>(),
    ) {
        return -2;
    }
    let (Some(a), Some(b)) = (unsafe { bignum_clone(a) }, unsafe { bignum_clone(b) }) else {
        return -2;
    };
    match RustCryptoBignum.cmp(&a, &b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

macro_rules! bignum_predicate_abi {
    ($name:ident, $method:ident) => {
        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name(value: *const BignumObject) -> c_int {
            unsafe { bignum_clone(value) }
                .is_some_and(|value| RustCryptoBignum.$method(&value))
                .into()
        }
    };
}

bignum_predicate_abi!(crypto_bignum_is_zero, is_zero);
bignum_predicate_abi!(crypto_bignum_is_one, is_one);
bignum_predicate_abi!(crypto_bignum_is_odd, is_odd);

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_bignum_legendre(
    value: *const BignumObject,
    prime: *const BignumObject,
) -> c_int {
    if !object_alias_is_valid(
        value.addr(),
        size_of::<BignumObject>(),
        prime.addr(),
        size_of::<BignumObject>(),
    ) {
        return -2;
    }
    let (Some(value), Some(prime)) = (unsafe { bignum_clone(value) }, unsafe {
        bignum_clone(prime)
    }) else {
        return -2;
    };
    match RustCryptoBignum.legendre(&value, &prime) {
        Ok(LegendreSymbol::NonResidue) => -1,
        Ok(LegendreSymbol::Zero) => 0,
        Ok(LegendreSymbol::Residue) => 1,
        Err(_) => -2,
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_init(group: c_int) -> *mut EcObject {
    let Ok(group_id) = u16::try_from(group) else {
        return ptr::null_mut();
    };
    let Ok(group) = RustCryptoGroup19::for_group(group_id) else {
        return ptr::null_mut();
    };
    let object = EcObject {
        magic: EC_MAGIC,
        group,
        prime: BignumObject::borrowed(group.prime()),
        order: BignumObject::borrowed(group.order()),
        coefficient_a: BignumObject::borrowed(group.coefficient_a()),
        coefficient_b: BignumObject::borrowed(group.coefficient_b()),
    };
    unsafe { allocate(object) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_deinit(pointer: *mut EcObject) {
    if unsafe { ec_group(pointer) }.is_some() {
        unsafe { deallocate(pointer) };
    }
}

macro_rules! ec_length_abi {
    ($name:ident, $value:expr) => {
        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name(context: *mut EcObject) -> usize {
            unsafe { ec_group(context) }.map_or(0, |_| $value)
        }
    };
}

ec_length_abi!(crypto_ec_prime_len, P256_ELEMENT_BYTES);
ec_length_abi!(crypto_ec_prime_len_bits, P256_ELEMENT_BYTES * 8);
ec_length_abi!(crypto_ec_order_len, P256_ELEMENT_BYTES);

macro_rules! ec_bignum_getter_abi {
    ($name:ident, $field:ident) => {
        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name(context: *mut EcObject) -> *const BignumObject {
            if unsafe { ec_group(context) }.is_none() {
                return ptr::null();
            }
            unsafe { ptr::addr_of!((*context).$field) }
        }
    };
}

ec_bignum_getter_abi!(crypto_ec_get_prime, prime);
ec_bignum_getter_abi!(crypto_ec_get_order, order);
ec_bignum_getter_abi!(crypto_ec_get_a, coefficient_a);
ec_bignum_getter_abi!(crypto_ec_get_b, coefficient_b);

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_init(context: *mut EcObject) -> *mut PointObject {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return ptr::null_mut();
    };
    unsafe {
        allocate(PointObject {
            magic: POINT_MAGIC,
            value: group.identity(),
        })
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_deinit(pointer: *mut PointObject, clear: c_int) {
    let _ = clear;
    if unsafe { point_clone(pointer) }.is_some() {
        // SaeP256Point zeroizes on every drop, independent of the clear hint.
        unsafe { deallocate(pointer) };
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_from_bin(
    context: *mut EcObject,
    encoded: *const u8,
) -> *mut PointObject {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return ptr::null_mut();
    };
    if encoded.is_null()
        || checked_range(encoded.addr(), P256_ELEMENT_BYTES * 2).is_none()
        || !matches!(
            ranges_overlap(
                encoded.addr(),
                P256_ELEMENT_BYTES * 2,
                context.addr(),
                size_of::<EcObject>(),
            ),
            Some(false)
        )
    {
        return ptr::null_mut();
    }
    let mut x = [0u8; P256_ELEMENT_BYTES];
    let mut y = [0u8; P256_ELEMENT_BYTES];
    unsafe {
        ptr::copy_nonoverlapping(encoded, x.as_mut_ptr(), P256_ELEMENT_BYTES);
        ptr::copy_nonoverlapping(
            encoded.add(P256_ELEMENT_BYTES),
            y.as_mut_ptr(),
            P256_ELEMENT_BYTES,
        );
    }
    let Ok(value) = group.point_from_xy(&x, &y) else {
        return ptr::null_mut();
    };
    unsafe {
        allocate(PointObject {
            magic: POINT_MAGIC,
            value,
        })
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_to_bin(
    context: *mut EcObject,
    point: *const PointObject,
    x: *mut u8,
    y: *mut u8,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return -1;
    };
    let Some(point_value) = (unsafe { point_clone(point) }) else {
        return -1;
    };
    for output in [x, y].into_iter().filter(|output| !output.is_null()) {
        if !matches!(
            ranges_overlap(
                output.addr(),
                P256_ELEMENT_BYTES,
                point.addr(),
                size_of::<PointObject>(),
            ),
            Some(false)
        ) || !matches!(
            ranges_overlap(
                output.addr(),
                P256_ELEMENT_BYTES,
                context.addr(),
                size_of::<EcObject>(),
            ),
            Some(false)
        ) {
            return -1;
        }
    }
    if !x.is_null()
        && !y.is_null()
        && !matches!(
            ranges_overlap(x.addr(), P256_ELEMENT_BYTES, y.addr(), P256_ELEMENT_BYTES),
            Some(false)
        )
    {
        return -1;
    }
    let Ok((x_value, y_value)) = group.point_to_xy(&point_value) else {
        return -1;
    };
    unsafe {
        if !x.is_null() {
            ptr::copy_nonoverlapping(x_value.as_ptr(), x, P256_ELEMENT_BYTES);
        }
        if !y.is_null() {
            ptr::copy_nonoverlapping(y_value.as_ptr(), y, P256_ELEMENT_BYTES);
        }
    }
    0
}

#[cfg(target_arch = "riscv32")]
fn point_aliases<const N: usize>(
    context: *const EcObject,
    output: *mut PointObject,
    inputs: [*const PointObject; N],
) -> bool {
    if output.is_null()
        || !matches!(
            ranges_overlap(
                output.addr(),
                size_of::<PointObject>(),
                context.addr(),
                size_of::<EcObject>(),
            ),
            Some(false)
        )
    {
        return false;
    }
    for (index, input) in inputs.iter().enumerate() {
        if input.is_null()
            || !object_alias_is_valid(
                output.addr(),
                size_of::<PointObject>(),
                input.addr(),
                size_of::<PointObject>(),
            )
        {
            return false;
        }
        for other in &inputs[..index] {
            if !object_alias_is_valid(
                input.addr(),
                size_of::<PointObject>(),
                other.addr(),
                size_of::<PointObject>(),
            ) {
                return false;
            }
        }
    }
    true
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_add(
    context: *mut EcObject,
    a: *const PointObject,
    b: *const PointObject,
    result: *mut PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return -1;
    };
    if !point_aliases(context, result, [a, b]) {
        return -1;
    }
    let (Some(a), Some(b)) = (unsafe { point_clone(a) }, unsafe { point_clone(b) }) else {
        return -1;
    };
    unsafe { point_store(result, group.point_add(&a, &b)) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_mul(
    context: *mut EcObject,
    point: *const PointObject,
    scalar: *const BignumObject,
    result: *mut PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return -1;
    };
    if !point_aliases(context, result, [point])
        || !matches!(
            ranges_overlap(
                result.addr(),
                size_of::<PointObject>(),
                scalar.addr(),
                size_of::<BignumObject>(),
            ),
            Some(false)
        )
        || !matches!(
            ranges_overlap(
                point.addr(),
                size_of::<PointObject>(),
                scalar.addr(),
                size_of::<BignumObject>(),
            ),
            Some(false)
        )
    {
        return -1;
    }
    let (Some(point), Some(scalar)) = (unsafe { point_clone(point) }, unsafe {
        bignum_clone(scalar)
    }) else {
        return -1;
    };
    let Ok(value) = group.point_mul(&point, &scalar) else {
        return -1;
    };
    unsafe { point_store(result, value) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_invert(
    context: *mut EcObject,
    point: *mut PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return -1;
    };
    if !point_aliases(context, point, [point]) {
        return -1;
    }
    let Some(value) = (unsafe { point_clone(point) }) else {
        return -1;
    };
    unsafe { point_store(point, group.point_invert(&value)) }.map_or(-1, |()| 0)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_compute_y_sqr(
    context: *mut EcObject,
    x: *const BignumObject,
) -> *mut BignumObject {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return ptr::null_mut();
    };
    let Some(x) = (unsafe { bignum_clone(x) }) else {
        return ptr::null_mut();
    };
    let Ok(value) = group.compute_y_squared(&x) else {
        return ptr::null_mut();
    };
    unsafe { allocate(BignumObject::owned(value)) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_is_at_infinity(
    context: *mut EcObject,
    point: *const PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return 0;
    };
    unsafe { point_clone(point) }
        .is_some_and(|point| group.point_is_infinity(&point))
        .into()
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_is_on_curve(
    context: *mut EcObject,
    point: *const PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return 0;
    };
    unsafe { point_clone(point) }
        .is_some_and(|point| group.point_is_on_curve(&point))
        .into()
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn crypto_ec_point_cmp(
    context: *const EcObject,
    a: *const PointObject,
    b: *const PointObject,
) -> c_int {
    let Some(group) = (unsafe { ec_group(context) }) else {
        return 1;
    };
    if !object_alias_is_valid(
        a.addr(),
        size_of::<PointObject>(),
        b.addr(),
        size_of::<PointObject>(),
    ) {
        return 1;
    }
    let (Some(a), Some(b)) = (unsafe { point_clone(a) }, unsafe { point_clone(b) }) else {
        return 1;
    };
    (!group.point_eq(&a, &b)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_checks_reject_partial_alias_and_wraparound() {
        assert!(object_alias_is_valid(0x1000, 64, 0x1000, 64));
        assert!(object_alias_is_valid(0x1000, 64, 0x1040, 64));
        assert!(!object_alias_is_valid(0x1000, 64, 0x1020, 64));
        assert_eq!(ranges_overlap(usize::MAX - 7, 8, 0x1000, 8), None);
    }

    #[test]
    fn entropy_mask_preserves_only_the_modulus_bit_width() {
        assert_eq!(entropy_top_mask(0), None);
        assert_eq!(entropy_top_mask(1), Some(0x01));
        assert_eq!(entropy_top_mask(0x7f), Some(0x7f));
        assert_eq!(entropy_top_mask(0x80), Some(0xff));
    }

    #[test]
    fn adapter_contract_is_group_19_only_and_round_trips_points() {
        assert!(RustCryptoGroup19::for_group(18).is_err());
        assert!(RustCryptoGroup19::for_group(20).is_err());
        let group = RustCryptoGroup19::for_group(GROUP_19).unwrap();
        let generator = group.generator();
        let (x, y) = group.point_to_xy(&generator).unwrap();
        let decoded = group.point_from_xy(&x, &y).unwrap();
        assert!(group.point_eq(&generator, &decoded));
        assert_eq!(x.len(), 32);
        assert_eq!(y.len(), 32);
    }
}

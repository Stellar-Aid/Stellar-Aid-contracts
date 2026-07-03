pub fn safe_add(a: i128, b: i128) -> i128 { a.checked_add(b).unwrap() }
pub fn safe_mul_2(a: i128, b: i128) -> i128 { a.checked_mul(b).unwrap() }

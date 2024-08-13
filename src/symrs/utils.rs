use num_integer;

pub fn lcm(a: u64, b: u64) -> u64 {
    a * b / num_integer::gcd(a, b)
}

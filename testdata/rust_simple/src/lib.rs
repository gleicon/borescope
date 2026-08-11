pub fn alpha() -> u32 {
    beta() + gamma()
}

pub fn beta() -> u32 {
    gamma() * 2
}

pub fn gamma() -> u32 {
    42
}

use std::array::IntoIter;
use smart_leds::RGB8;

pub struct Ws2812Rpi();

impl Ws2812Rpi {
    pub fn new(_a:i32,_b:i32) -> Option<Self> {
        Some(Self())
    }

    pub fn write<const N: usize>(&mut self, _data: IntoIter<RGB8,N>) -> Option<()> {
        Some(())
    }
}
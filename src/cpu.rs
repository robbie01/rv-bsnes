#![expect(dead_code)]

const CANONICAL_NAN_F32: f32 = f32::from_bits(0x7fc00000);
const CANONICAL_NAN_F64: f64 = f64::from_bits(0x7ff8000000000000);

#[derive(Clone, Copy)]
struct FRegister {
    value: f64
}

impl FRegister {
    fn read_f64(self) -> f64 {
        self.value
    }

    fn read_f32(self) -> f32 {
        let bits = self.value.to_bits();
        let box_ = (bits >> 32) as u32;
        if box_ == u32::MAX {
            f32::from_bits(bits as u32)
        } else {
            CANONICAL_NAN_F32
        }
    }

    fn write_f64(value: f64) -> Self {
        Self { value }
    }

    fn write_f32(value: f32) -> Self {
        Self {
            value: f64::from_bits((u64::MAX << 32) | u64::from(value.to_bits()))
        }
    }
}
use crate::acoustic_modem::demodulation::{self, Demodulation};

#[test]
fn test_demodulation() {
    let mut demodulator = Demodulation::new(vec![1000], 48000, false);
}
/*
Input data
-> Modulation
-> Output Signal
*/

const sampleRate: i32 = 48000;
const leastCarrierFrequency: i32 = 2000;

pub struct OutputWave {
    carrierFrequency: i32,
    onePeriodVoltage: Vec<i32>,
}

impl OutputWave {
    pub fn new(carrierFrequency: i32, sampleRate: i32) -> Self {
        let onePeriodLength = sampleRate / carrierFrequency;
        let onePeriodVoltage = Vec::new();
        return Self {
            carrierFrequency,
            onePeriodVoltage,
            sampleRate,
        };
    }
}

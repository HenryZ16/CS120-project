# CS120-PROJECT

## Modulation Specification

- Shift keying policy:

    **PSK** (Phase Shift Keying).

- Carrier frequency: **1000Hz**

    This frequency is low enough to come across the obstacles. Also it can avoid the inaccuracy bringing from the non-differential point when we using PSK.

## PHY Frame Specification

The length of our PHY Frame is 1064, where each part is:

`[Preamble : 10][Length : 30][Payload : 1024]`

- `Preamble`: when a receiver detects this pattern, it means that we receives a frame. The preamble follows this pattern:

    `0101010101` (5 `01`s)

    We can use a state machine to detect this preamble.

    For why we use this pattern, you can assume that the nature generates `0` and `1` randomly at a probability of `0.5`. The probability for nature to generate `0101010101` is lower than `1e-4`, which is acceptable, and you can use Markov chain to prove it.

- `Length`: this part indicates the length of the payload. actually, a payload with maximum length of 1024 only needs `10` bits to store its length. Here we use more 2 redundant bits for each original bits, to keep the accuracy.

    Since we use Reed-Solomon to encode the payload, `Length` here stores the original (before encoding) length of the data.

- `Payload`: as mentioned in `Length`, we use Reed-Solomon to encode the data, to ensure the accuracy. In each payload, we use 64 bits to correct 32 symbolic errors. So the maximum length of the data in payload is 960 bits.
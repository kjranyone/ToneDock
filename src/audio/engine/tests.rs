use super::input_fifo::InputFifo;

#[test]
fn input_fifo_reblocks_input_for_output() {
    let mut fifo = InputFifo::new(2, 32, 4);

    fifo.push_interleaved(&[1.0, 10.0, 2.0, 20.0], 2);
    fifo.push_interleaved(&[3.0, 30.0, 4.0, 40.0], 2);

    let mut output = vec![0.0f32; 4];
    fifo.pop_mono_into(0, &mut output, 1.0);

    assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn input_fifo_keeps_channels_aligned_when_trimming() {
    let mut fifo = InputFifo::new(2, 3, 1);
    fifo.push_interleaved(&[1.0, 10.0, 2.0, 20.0, 3.0, 30.0, 4.0, 40.0], 2);

    let mut output = vec![0.0f32; 3];
    fifo.pop_mono_into(1, &mut output, 1.0);

    assert_eq!(output, vec![20.0, 30.0, 40.0]);
}

#[test]
fn input_fifo_rebalances_latency_before_output() {
    let mut fifo = InputFifo::new(1, 16, 2);
    fifo.push_interleaved(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 1);

    let mut output = vec![0.0f32; 2];
    fifo.pop_mono_into(0, &mut output, 1.0);

    assert_eq!(output, vec![3.0, 4.0]);
}

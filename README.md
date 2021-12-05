# Preprocess MIDI files into performance events

This Rust executable converts MIDI files into pytorch tensors using the performance representation described in the PerformanceRNN paper [1]. It was written specifically to preprocess the files in the [MAESTRO dataset](https://magenta.tensorflow.org/datasets/maestro) [2].

Usage:

```
cargo run [input_directory] [output_directory] 
```

Example: `cargo run ~/Downloads/maestro-v3.0.0 output/maestro-v3.0.0`

This will recursively traverse the input directory, convert all MIDI files into tensors, and then write them out to the output directory in the same file system structure. They can be then read in pytorch like this:

```
import torch
v = torch.jit.load('file.midi.pt').state_dict()['0']
```

Each output file is an i16 1D tensor, where each value has the following meaning:

* 0-127 : `NOTE_ON`
* 128-255 : `NOTE_OFF`
* 256-355 : `DELTA_TIME`
* 356-387 : `VELOCITY_CHANGE`

The `NOTE_ON` and `NOTE_OFF` events are the same as [MIDI note values](https://www.inspiredacoustics.com/en/MIDI_note_numbers_and_center_frequencies), so an element with value 60 indicates a `NOTE_ON` event turning on Middle C (C4), and an element with value `128 + 60 = 188` indicates a `NOTE_OFF` event turning C4 off.

`DELTA_TIME` events are discretized with 10 ms granularity. For example, an element with value "300" indicates a time delay of `(300 - 256 + 1) * 10 ms = 450 ms`. Time delays that are greater than 1 second are split up into multiple consecutive `DELTA_TIME` events, so a delay of 2450 ms is represented as three events: `[355, 355, 300]`.

`VELOCITY_CHANGE` events indicates volume/emphasis extracted from the MIDI note events, then divided by 4.

```
[1] Ian Simon and Sageev Oore. "Performance RNN: Generating Music with Expressive
  Timing and Dynamics." Magenta Blog, 2017.
  https://magenta.tensorflow.org/performance-rnn

[2] Curtis Hawthorne, Andriy Stasyuk, Adam Roberts, Ian Simon, Cheng-Zhi Anna Huang,
  Sander Dieleman, Erich Elsen, Jesse Engel, and Douglas Eck. "Enabling
  Factorized Piano Music Modeling and Generation with the MAESTRO Dataset."
  In International Conference on Learning Representations, 2019.
```

# midi-arpeggiator
A CLI tool which reads held notes from MIDI-IN and arpeggiates them to MIDI-OUT

There are a number of types of arpeggiator:
- RepeatRecorder: Hold down notes in the order (and timing) they should be arpeggiated then release and re-play the first note to trigger arpeggiation. The arpeggio is played until the note used to trigger is released.
- TimedPedalRecorder: Play notes in the order (and timing) they should be arpeggiated while holding down the damper pedal. When the damper pedal is released, the notes and timing between the first note down and the pedal release is arpeggiated. The arpeggio will be stopped when the first note of the arpeggio is release (it can be safely pressed at any time and will not be passed through to MIDI-OUT). The same arpeggio can be replayed in the same or different key by pressing and holding the note it should start on.
- PressHold: Hold down the notes (at once) which should be arpeggiated. They will be split into steps based on the `fixed_steps` setting if set, falling back to the `fixed_notes_per_step` setting. The order of the notes is determined by the `pattern` setting. If more notes are required than supplied, extra notes are generated by repeating all except the first and last notes in reverse order (ie. 'up' pattern becomes 'up/down', 'down' becomes 'down/up'). The arpeggio is stopped when all notes in the arpeggio are released. The arpeggio steps will be spaced evenly into 1 quarter note as per the MIDI clock-ticks being sent by the MIDI-OUT device.
- MutatingHold: Hold down the notes (in order) which should be arpeggiated. Holding additional notes will update the arpeggio (without stopping it) so that extra notes can be added to the end of the arpeggio. Released notes will be removed from the arpeggio when the next update is trigged by holding an additional note. The playing arpeggio play back at 1 step per quarter note as per the MIDI clock-ticks being sent by the MIDI-OUT device, and will be stopped when all notes are released. Only one arpeggio is possible at a time in this mode.
- SyncedPedalRecorder: Similar to TimedPedalRecorder, except the timing is not recorded, just the notes. The recorded steps are then arpeggiated at 1 step per quarter note as per the MIDI clock-ticks being sent by the MIDI-OUT device.

For simplicity, the current CLI interface takes only 1 argument, the path to the SETTINGS file, which defaults to `settings.json`. The SETTINGS file is expected to be valid json representing a single or comma separated list of the settings objects described below. For ease of use, the surrounding `[` and `]` are implied and should not be included in the file.
```
{
    "finish_pattern": true/false, // determines if the arpeggio finishes playing its full set of steps (true), or stops immediately (false)
    "mode": "RepeatRecorder"/"TimedPedalRecorder"/"PressHold"/"MutatingHold"/"SyncedPedalRecorder", // as above
    "pattern": "Up"/"Down", // determines the order the notes are played in the arpeggio
    "fixed_steps": 4, // optional, if set it must be a positive integer determining how many steps to divide the notes into
    "fixed_notes_per_step": 1, // optional, if set it must be a positive integer determining how many notes to allocate to each step
    "fixed_velocity": 0-127 // optional, if set it determines the velocity of the notes played back in arpeggios, otherwise the recored velocity is used
}
```

The MIDI-IN and MIDI-OUT devices are determined as follows:
- A list of MIDI devices is found from `/dev/midi*`
- If 1 device is found, it is used as both the MIDI-IN and MIDI-OUT device
- If 2 devices are found, then the first one which is sending a MIDI clock-tick is used as MIDI-OUT, with the other as MIDI-IN
- If 3 or more devices are found (or none), then the arpeggiator will exit with an error

In order to use multiple types of arpeggiation, the arpeggiator listen to MIDI program changes (0-127) matching the index of the settings object in the SETTINGS file.

For instructions on how to run this on a Raspberry Pi 0w, click [here](hardware/SETUP.md).
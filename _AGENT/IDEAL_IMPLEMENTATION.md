Here’s what the _low-level_ architecture usually looks like on macOS when you need:

- **Screen video**
- **System audio**
- **Microphone audio**
- **Webcam overlay (picture-in-picture)**
- **Live controls** (mute mic only / mute system only / hide/show camera overlay)
- **Perfect sync** in the final file

The key is: **everything must live on one timeline** and you must be very disciplined about **timestamps**, **sample rates**, and **when you drop vs. silence**.

---

## 1) What you use on macOS (and why)

### Screen + system audio

Use **ScreenCaptureKit** (macOS 12.3+). It’s Apple’s modern high-performance capture API. It delivers **both video frames and audio samples as `CMSampleBuffer`** objects via the stream output callback. ([Apple Developer][1])
System audio comes through the stream as output type **`.audio`**, again as `CMSampleBuffer` containing an `AudioBufferList`. ([Apple Developer][2])

### Webcam video

Use **AVFoundation camera capture** (`AVCaptureSession` + `AVCaptureVideoDataOutput`) to receive **webcam frames as `CMSampleBuffer`**. (This is the “normal” way to get camera frames on macOS.)

### Microphone audio

Use either:

- **AVAudioEngine** input node tap (convenient, integrates mixing/resampling), or
- **Core Audio / AudioUnit (HAL)** for tighter control and device-level clocking (more work, more deterministic).

AVAudioEngine taps are a common capture primitive for mic PCM. ([Stack Overflow][3])

### Encoding + container writing

For “perfect sync” and a straightforward pipeline, use **AVAssetWriter** (hardware encoding, stable timestamps model, plays nicely with `CMSampleBuffer` time).
You _can_ use FFmpeg, but then you’re responsible for far more details (timebases, drift correction, resampling, muxing quirks, VT compression integration, etc.). In practice, many macOS recorders do **capture with Apple APIs** and **write with AVAssetWriter**.

---

## 2) Big-picture dataflow (what happens every millisecond)

Think of four producers and one recorder:

**Producers**

1. ScreenCaptureKit → screen video buffers (`CMSampleBuffer`)
2. ScreenCaptureKit → system audio buffers (`CMSampleBuffer`)
3. AVAudioEngine / AudioUnit → mic PCM buffers (you convert/wrap into `CMSampleBuffer` or otherwise feed a writer/mixer)
4. AVCaptureSession → camera video buffers (`CMSampleBuffer`)

**Recorder pipeline**

- A **sync/timeline authority**
- A **video compositor** (screen + optional camera overlay) → “final video frames”
- An **audio mixer** (system + mic, independently mutable) → “final audio stream”
- **AVAssetWriter** writing 1 video track + 1 audio track into MP4/MOV

The trick is: you never want “four independent recordings”. You want **one timeline** with **one video stream** and **one audio stream** that you continuously append.

---

## 3) The timeline problem (the real “low-level” heart)

### `CMSampleBuffer` timestamps are everything

Every `CMSampleBuffer` has a **presentation timestamp (PTS)**. For sync you do all alignment based on these PTS values. Apple exposes this concept directly (e.g. `CMSampleBufferGetPresentationTimeStamp`). ([Apple Developer][4])

### Pick a single “start time” (time zero)

At record start, the capture system won’t hand you perfectly simultaneous first buffers.

So you establish:

- **`t0` = timestamp of the first _accepted_ video frame** (common choice), or sometimes the earliest timestamp seen across streams.

Then you normalize:

- `normalizedPTS = samplePTS - t0`

Everything you write to the file is in this normalized timeline.

### Start the writer session exactly once

`AVAssetWriter` needs a session start time (`startSession(atSourceTime:)`) and it must match your chosen timeline base, otherwise you get drift, offsets, or “cannot append… must start session” style failures. ([Stack Overflow][5])

---

## 4) How to produce the final video (screen + overlay that can be toggled)

You have two viable designs:

### Design A (most common for your requirements): **Real-time compositing → single video track**

1. Screen video frame arrives (pixel buffer inside the sample).
2. Camera frame arrives independently.
3. A compositor produces one output frame for each screen frame:

   - Base layer: screen
   - Overlay: camera (if enabled), positioned/scaled, maybe rounded corners/shadow

4. Output is a new `CVPixelBuffer` with **the screen frame’s timestamp** (this is important).
5. Append that to the video input of `AVAssetWriter`.

**Hide/show overlay while recording**

- This becomes trivial: the compositor has a boolean `overlayEnabled`.
- When false: it simply doesn’t draw the camera layer, but still outputs frames at the same cadence/timestamps.
- The final file reflects the change instantly, with no timeline discontinuity.

This is the “you want it to reflect in the final output” friendly approach.

### Design B: Store separate video tracks and “compose later”

You write:

- Track 1 = screen video
- Track 2 = camera video

Then you do an export/composition step after stop. This makes live “hide/show overlay” harder to reflect exactly as the user saw it (you must record an event timeline and apply it during export), and it’s slower / more complex. It’s usually not worth it if you need WYSIWYG recording.

**So for your app: Design A is the practical choice.**

---

## 5) How to produce the final audio (system + mic with independent mute)

Again, two common approaches:

### Approach A (recommended): **Real-time mixing → single audio track**

- System audio buffers come from ScreenCaptureKit as `CMSampleBuffer` `.audio`. ([Apple Developer][2])
- Mic audio comes from AVAudioEngine/AudioUnit as PCM.

You feed both into an **audio mixer** that outputs:

- A single stream, fixed format (e.g., 48 kHz, 2ch float/16-bit PCM)
- With stable timestamps on the same normalized timeline

Then you append that mixed stream to the audio input of `AVAssetWriter`.

**Mute mic only / mute system only**
You do _not_ want to stop time or create gaps.
You change gains:

- `micGain = 0` to mute mic
- `systemGain = 0` to mute system

The mixer still outputs continuous audio buffers at the same cadence.
That guarantees:

- no “audio track ends early”
- no drifting A/V sync
- no weird silence gaps with missing timestamps

### Approach B: Two audio tracks then mix later

You can write two separate audio tracks (system + mic) and later mix/export. Same drawbacks as video: you now need post-processing to reflect live mute toggles (you’d apply automation curves after the fact). It can work, but it’s more moving parts.

**For “live toggles reflected in final output” + “perfect sync”: mix live.**

---

## 6) The hard part: keeping audio locked (no drift)

Even with timestamps, audio can drift if:

- mic capture clock and system audio capture clock aren’t the same
- sample rates don’t match
- one stream occasionally stalls and you “drop” buffers incorrectly

### What “perfectly synced” usually means in practice

You pick the **video timeline** (screen frames) as the master clock and you ensure audio output matches it.

To do that reliably, your audio mixer typically must:

1. **Resample** one or both inputs to a common sample rate (very often 48k).
2. **Align** buffers by their timestamps.
3. **Fill** missing ranges with silence (not by skipping time).
4. **Optionally time-stretch very slightly** (tiny corrections) if you detect drift over long captures.

ScreenCaptureKit audio is already timestamped in the same conceptual media time domain as its video (it’s coming out of the same stream). Your mic is the “external clock” that’s most likely to drift relative to that.

So the mixer often treats:

- system audio timestamps = authoritative
- mic audio = resampled/shifted to match

(If you go deeper with Core Audio HAL you can sometimes do tighter clock alignment, but the conceptual solution stays the same: **one master timeline + drift correction**.)

---

## 7) Control events during recording (mute/hide) and how they must affect output

You will maintain a single “recording state” that is read by the audio mixer + video compositor:

- `overlayEnabled` (bool)
- `micGain` (float)
- `systemGain` (float)

When the user toggles something:

- You **do not** restart sessions.
- You **do not** reset timestamps.
- You **do not** open new files.
- You just update state, and the next produced buffers reflect it.

### Important detail: don’t “drop” audio buffers to mute

If you simply stop appending mic buffers, you create:

- discontinuities
- timestamp jumps
- in some cases writer backpressure issues
- audible pops when reintroducing audio

Muting = produce audio samples with gain=0 (silence), with correct timestamps.

---

## 8) Concurrency and backpressure (what can silently break sync)

This kind of recorder usually has:

- capture callbacks on system threads (ScreenCaptureKit + AVCapture + audio)
- a processing graph (compositor + mixer)
- a writing queue (AVAssetWriter inputs can apply backpressure)

To avoid stalls:

- You typically decouple capture from encode/write using ring buffers/queues.
- If writer can’t keep up, you must decide:

  - drop video frames (while keeping timestamps monotonic), or
  - reduce complexity, or
  - apply adaptive quality/bitrate

But **never** let audio and video advance on different “ideas of time”.

---

## 9) Where Tauri + Rust fits (practically)

Tauri/Rust is great for:

- UI + state management
- configuration
- file management
- IPC orchestration
- performance-critical non-AV logic

But the actual capture/composition/writing stack is best done with **native macOS frameworks**, which means you typically have:

- A **macOS native module** (Swift/ObjC) that talks to ScreenCaptureKit / AVFoundation / CoreMedia
- Expose a thin command interface to Rust via Tauri (start/stop, set gains, toggle overlay, select devices, etc.)
- Rust is the “controller”, native is the “real-time media engine”

You _can_ call some of these via Rust bindings, but the reality is: the most stable path is still a native layer for AV plumbing, because you’re living in CoreMedia/CoreAudio types and callbacks.

---

## 10) Summary: the clean “one-pass” architecture that satisfies all constraints

**Capture**

- Screen + system audio: ScreenCaptureKit → timestamped `CMSampleBuffer` video + audio ([Apple Developer][1])
- Camera: AVCaptureSession → `CMSampleBuffer` video
- Mic: AVAudioEngine/AudioUnit → PCM buffers (convert to a uniform format)

**Normalize timeline**

- Choose `t0`
- Normalize all PTS = `PTS - t0`
- Start AVAssetWriter session at 0 (or at `t0` in source time, consistently)

**Process**

- Video compositor: screen frame cadence drives output; draws camera only if enabled
- Audio mixer: continuous output, system+mic gains applied; resample/align/fill silence; avoid time discontinuities

**Write**

- AVAssetWriter video input: append composited frames with the screen timestamps
- AVAssetWriter audio input: append mixed audio buffers with continuous timestamps
- Stop: mark inputs finished, finalize file

That design makes your live controls trivial, produces a single final file with no “post step”, and gives you the best chance at truly tight sync.

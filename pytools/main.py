import subprocess
import cv2
import numpy as np
import argparse
import rich.status


# ── Pure helpers (no side effects, trivially testable) ────────────────────────

def to_rgb332(r: int, g: int, b: int) -> int:
    """Encode an 8-bit RGB triplet as a single RGB332 byte."""
    return ((r >> 5) & 0b111) << 5 | ((g >> 5) & 0b111) << 2 | ((b >> 6) & 0b11)


def encode_pixel(index: int, r: int, g: int, b: int) -> str:
    """Return BF instructions that write rgb332 into the cell at `index`."""
    move = ">" if index > 0 else ""
    return f"{move}[-]{'+'  * to_rgb332(r, g, b)}"


def extract_audio(video_path: str, sample_rate: int) -> np.ndarray:
    """Extract mono float32 audio from a video file via ffmpeg."""
    cmd = [
        "ffmpeg", "-i", video_path,
        "-f", "f32le", "-ac", "1", "-ar", str(sample_rate),
        "-loglevel", "error", "pipe:1",
    ]
    result = subprocess.run(cmd, capture_output=True)
    return np.frombuffer(result.stdout, dtype=np.float32)


def samples_to_note(chunk: np.ndarray, sample_rate: int) -> int:
    """
    Detect the dominant pitch in an audio chunk and return a MIDI note (0 = silence).
    Uses a Hann-windowed FFT limited to the musical range 80–2000 Hz.
    """
    if len(chunk) == 0 or np.max(np.abs(chunk)) < 0.01:
        return 0
    windowed = chunk * np.hanning(len(chunk))
    fft      = np.abs(np.fft.rfft(windowed))
    freqs    = np.fft.rfftfreq(len(chunk), 1.0 / sample_rate)
    mask     = (freqs >= 80) & (freqs <= 2000)
    if not mask.any():
        return 0
    peak_freq = freqs[mask][np.argmax(fft[mask])]
    midi = round(12 * np.log2(peak_freq / 440.0) + 69)
    return int(np.clip(midi, 0, 127))


def encode_frame(flat_pixels, total_pixels: int, note: int) -> str:
    """
    Write pixels into RAM cells 0..total_pixels-1, step into cell `total_pixels`
    (audio zone, outside the display area), write the note, emit `.`, rewind to 0.

    The Rust viewer reads ram[ptr] after `.` as the audio pitch, so the pointer
    must sit in the audio cell — not the pixel area — when `.` fires.
    """
    pixel_ops = "".join(
        encode_pixel(i, r, g, b)
        for i, (r, g, b) in enumerate(flat_pixels)
    )
    return (
        pixel_ops
        + ">"                    # step into audio cell (index total_pixels)
        + "[-]" + "+" * note     # write note value
        + "."                    # sync output; ram[ptr] = note → audio
        + "<" * total_pixels     # rewind to cell 0 for next frame
    )


# ── Generator: side-effect boundary for video + audio I/O ────────────────────

def iter_frames(cap: cv2.VideoCapture, fps: float, frame_skip: int,
                frame_size: tuple, audio: np.ndarray, sample_rate: int):
    """Yield (flat_pixels, note) for every encoded frame."""
    samples_per_video_frame = sample_rate / fps
    frame_count = 0
    while True:
        ret, frame = cap.read()
        if not ret:
            break
        if frame_count % frame_skip == 0:
            frame    = cv2.resize(frame, frame_size, interpolation=cv2.INTER_AREA)
            pixels   = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB).reshape(-1, 3)
            start    = int(frame_count * samples_per_video_frame)
            end      = int(start + samples_per_video_frame * frame_skip)
            note     = samples_to_note(audio[start:end], sample_rate)
            yield pixels, note
        frame_count += 1


# ── Main class ────────────────────────────────────────────────────────────────

class vidfack:
    """
    Convert video frames into a Brainfuck (.b) program using RGB332 colour encoding.
    Audio is extracted from the video and pitch-detected per frame; the dominant
    frequency is written as a MIDI note into the audio cell (just past the pixel area).

    Args:
        video_path:   Path to the input video file.
        out_b_file:   Path to the output .b file.
        frame_size:   Target (width, height) per frame. Defaults to (16, 16).
        fps_limit:    Maximum frames per second to encode. Defaults to 60.
        sample_rate:  Audio sample rate passed to ffmpeg. Defaults to 48000.
    """

    def __init__(
        self,
        video_path:  str,
        out_b_file:  str,
        frame_size:  tuple = (16, 16),
        fps_limit:   int   = 60,
        sample_rate: int   = 48000,
    ):
        self.video_path  = video_path
        self.out_b_file  = out_b_file
        self.frame_size  = frame_size
        self.fps_limit   = fps_limit
        self.sample_rate = sample_rate

    def generate(self) -> None:
        cap = cv2.VideoCapture(self.video_path)
        if not cap.isOpened():
            raise IOError(f"Cannot open video file: {self.video_path}")

        fps          = cap.get(cv2.CAP_PROP_FPS) or 30
        frame_skip   = max(round(fps / self.fps_limit), 1)
        total_pixels = self.frame_size[0] * self.frame_size[1]
        audio        = extract_audio(self.video_path, self.sample_rate)

        try:
            bf_program = "".join(
                encode_frame(pixels, total_pixels, note)
                for pixels, note in iter_frames(
                    cap, fps, frame_skip, self.frame_size, audio, self.sample_rate
                )
            )
        finally:
            cap.release()

        with open(self.out_b_file, "w") as f:
            f.write(bf_program)

        print(f"✅ Saved RGB332 .b file to: {self.out_b_file}")


# ── CLI ───────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Convert video frames to a BF .b program.")
    parser.add_argument("video_path",    help="Path to the input video file.")
    parser.add_argument("output_b_file", help="Path to the output .b file.")
    parser.add_argument("--width",       type=int, default=16,    help="Frame width (default: 16).")
    parser.add_argument("--height",      type=int, default=16,    help="Frame height (default: 16).")
    parser.add_argument("--fps_limit",   type=int, default=10,    help="FPS limit (default: 10).")
    parser.add_argument("--sample_rate", type=int, default=48000, help="Audio sample rate (default: 48000).")
    args = parser.parse_args()

    with rich.status.Status("Processing video...", spinner="dots") as status:
        status.start()
        vidfack(
            video_path=args.video_path,
            out_b_file=args.output_b_file,
            frame_size=(args.width, args.height),
            fps_limit=args.fps_limit,
            sample_rate=args.sample_rate,
        ).generate()
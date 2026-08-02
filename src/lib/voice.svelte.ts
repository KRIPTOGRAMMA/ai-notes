// Shared dictation state: one recording for the whole window, driven either by the
// microphone button or by the Ctrl+Shift+D hotkey.
//
// The state lives here rather than inside VoiceButton because both entry points
// must mean the same thing. With a copy per component the hotkey could start a
// recording the button believes is not running, and the second press would ask the
// backend to stop something it never started.
//
// There is at most one recording by design — the backend rejects a second start,
// and dictating into two places at once has no meaning anyway.

import { api } from "./api/tauri";

class VoiceRecorder {
  /// Whether voice input can run at all: model plus sidecar. Checked once.
  available = $state(false);
  recording = $state(false);
  /// True while the backend is starting or transcribing — the slow half.
  busy = $state(false);
  error: string | null = $state(null);

  #checked = false;

  /// Resolves availability once per window; repeated calls are free.
  async ensureChecked() {
    if (this.#checked) return this.available;
    this.#checked = true;
    this.available = await api.voiceAvailable().catch(() => false);
    return this.available;
  }

  /// Starts or stops depending on the current state, returning the recognised text
  /// when a recording has just finished. Returns null in every other case, so the
  /// caller can insert unconditionally.
  async toggle(): Promise<string | null> {
    if (this.busy) return null;
    this.error = null;

    if (!this.recording) {
      this.busy = true;
      try {
        await api.startVoiceRecording();
        this.recording = true;
      } catch (e) {
        this.error = String(e);
      } finally {
        this.busy = false;
      }
      return null;
    }

    // Stopping is the slow half: whisper runs over the whole phrase here. The flag
    // is cleared first so the indicator stops pulsing while the text is decoded.
    this.busy = true;
    this.recording = false;
    try {
      const text = await api.stopVoiceRecording();
      return text || null;
    } catch (e) {
      this.error = String(e);
      return null;
    } finally {
      this.busy = false;
    }
  }
}

export const voice = new VoiceRecorder();

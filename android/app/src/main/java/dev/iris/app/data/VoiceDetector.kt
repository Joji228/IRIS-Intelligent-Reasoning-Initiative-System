package dev.iris.app.data

import com.konovalov.vad.webrtc.VadWebRTC
import com.konovalov.vad.webrtc.config.FrameSize
import com.konovalov.vad.webrtc.config.Mode
import com.konovalov.vad.webrtc.config.SampleRate

/**
 * Says, 50 times a second, whether a 20 ms frame of microphone audio holds a human voice.
 * Calls only talk to this interface, so the detector can later be swapped for a more accurate
 * one (Silero is better in noise) without touching the call code.
 */
interface VoiceDetector : AutoCloseable {
    /** [frame] is [Recorder.FRAME_SAMPLES] samples of 16 kHz mono audio. */
    fun isSpeech(frame: ShortArray): Boolean
}

/** WebRTC's voice activity detector, in its strictest mode (it ignores most background noise). */
class WebRtcVoiceDetector : VoiceDetector {
    private val vad = VadWebRTC(
        sampleRate = SampleRate.SAMPLE_RATE_16K,
        frameSize = FrameSize.FRAME_SIZE_320,
        mode = Mode.VERY_AGGRESSIVE,
        silenceDurationMs = 0,
        speechDurationMs = 0,
    )

    override fun isSpeech(frame: ShortArray): Boolean = vad.isSpeech(frame)

    override fun close() = vad.close()
}

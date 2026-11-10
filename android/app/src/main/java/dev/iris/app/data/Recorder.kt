package dev.iris.app.data

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Turns raw 16-bit mono audio into a WAV file (a 44-byte header plus the samples). Groq's
 * speech to text takes WAV as it is. A 16 kHz clip is about 32 KB per second.
 */
object Wav {
    const val MIME = "audio/wav"

    fun encode(samples: ShortArray, sampleRate: Int = Recorder.SAMPLE_RATE): ByteArray {
        val dataSize = samples.size * 2
        val out = ByteBuffer.allocate(44 + dataSize).order(ByteOrder.LITTLE_ENDIAN)
        out.put("RIFF".toByteArray(Charsets.US_ASCII))
        out.putInt(36 + dataSize)
        out.put("WAVE".toByteArray(Charsets.US_ASCII))
        out.put("fmt ".toByteArray(Charsets.US_ASCII))
        out.putInt(16) // size of the format block
        out.putShort(1) // plain PCM
        out.putShort(1) // one channel
        out.putInt(sampleRate)
        out.putInt(sampleRate * 2) // bytes per second
        out.putShort(2) // bytes per sample frame
        out.putShort(16) // bits per sample
        out.put("data".toByteArray(Charsets.US_ASCII))
        out.putInt(dataSize)
        for (s in samples) out.putShort(s)
        return out.array()
    }
}

/**
 * The phone's microphone as a stream of 20 ms frames of 16 kHz mono audio. It records with
 * the "voice communication" source, so Android applies its own echo cancellation and noise
 * suppression where the phone has them (they are also switched on explicitly below).
 * The same recorder serves Settings → Voice & calls ("Try it") and, later, the call screen.
 */
class Recorder {
    private var record: AudioRecord? = null
    private var echo: AcousticEchoCanceler? = null
    private var noise: NoiseSuppressor? = null

    /** Opens the microphone; false if it couldn't (no permission, or another app has it). */
    @SuppressLint("MissingPermission") // checked by the caller with [hasPermission]
    fun open(): Boolean {
        if (record != null) return true
        val minBuffer = AudioRecord.getMinBufferSize(SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (minBuffer <= 0) return false
        val r = try {
            AudioRecord(
                MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
                // Room for about half a second, so a slow moment never drops audio.
                maxOf(minBuffer, SAMPLE_RATE),
            )
        } catch (e: Exception) {
            return false
        }
        if (r.state != AudioRecord.STATE_INITIALIZED) {
            r.release()
            return false
        }
        if (AcousticEchoCanceler.isAvailable()) echo = runCatching { AcousticEchoCanceler.create(r.audioSessionId)?.also { it.enabled = true } }.getOrNull()
        if (NoiseSuppressor.isAvailable()) noise = runCatching { NoiseSuppressor.create(r.audioSessionId)?.also { it.enabled = true } }.getOrNull()
        record = r
        return true
    }

    fun start(): Boolean {
        val r = record ?: return false
        return runCatching { r.startRecording() }.isSuccess && r.recordingState == AudioRecord.RECORDSTATE_RECORDING
    }

    /** Fills [frame] with the next samples (blocks until they're there); the count read, or -1 on an error. */
    fun read(frame: ShortArray): Int {
        val r = record ?: return -1
        var got = 0
        while (got < frame.size) {
            val n = r.read(frame, got, frame.size - got)
            if (n < 0) return -1
            got += n
        }
        return got
    }

    fun stop() {
        runCatching { record?.stop() }
    }

    fun close() {
        stop()
        runCatching { echo?.release() }
        runCatching { noise?.release() }
        runCatching { record?.release() }
        echo = null
        noise = null
        record = null
    }

    companion object {
        /** What Whisper works best with, and what the voice detector needs. */
        const val SAMPLE_RATE = 16_000

        /** 20 ms of audio: the size the voice detector works in. */
        const val FRAME_SAMPLES = 320

        fun hasPermission(context: Context) =
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

        /**
         * Records one clip: from now until [shouldStop] says so or [maxMs] has passed. Returns
         * the audio as a WAV, or null if the microphone couldn't be opened.
         */
        suspend fun recordClip(maxMs: Long, shouldStop: () -> Boolean): ByteArray? = withContext(Dispatchers.IO) {
            val recorder = Recorder()
            try {
                if (!recorder.open() || !recorder.start()) return@withContext null
                val pcm = ByteArrayOutputStream()
                val frame = ShortArray(FRAME_SAMPLES)
                val bytes = ByteBuffer.allocate(frame.size * 2).order(ByteOrder.LITTLE_ENDIAN)
                val deadline = System.currentTimeMillis() + maxMs
                while (!shouldStop() && System.currentTimeMillis() < deadline) {
                    if (recorder.read(frame) < 0) break
                    bytes.clear()
                    for (s in frame) bytes.putShort(s)
                    pcm.write(bytes.array())
                }
                val raw = pcm.toByteArray()
                val samples = ShortArray(raw.size / 2)
                ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(samples)
                Wav.encode(samples)
            } finally {
                recorder.close()
            }
        }
    }
}

package dev.iris.app.data

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The phone's audio set up like a phone call, and put back afterwards: "in communication" mode
 * (which turns on the hardware echo path), audio focus for voice, and the route: earpiece by
 * default, the loudspeaker on request, a Bluetooth headset when one is connected. While the
 * sound is at the ear, the proximity sensor turns the screen off.
 */
class CallAudio(context: Context) {
    private val appContext = context.applicationContext
    private val audio = appContext.getSystemService(AudioManager::class.java)
    private val power = appContext.getSystemService(PowerManager::class.java)
    private var focus: AudioFocusRequest? = null
    private var previousMode = AudioManager.MODE_NORMAL
    private var proximity: PowerManager.WakeLock? = null
    private var devices: AudioDeviceCallback? = null
    private var loudspeaker = false
    private var started = false

    private val current = MutableStateFlow(AudioRoute.Earpiece)

    /** Where the sound goes now. */
    val route: StateFlow<AudioRoute> = current

    fun start(loudspeakerFirst: Boolean) {
        if (started) return
        started = true
        loudspeaker = loudspeakerFirst
        previousMode = audio.mode
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build(),
            )
            .setOnAudioFocusChangeListener { }
            .build()
        runCatching { audio.requestAudioFocus(request) }
        focus = request
        // A headset connecting or leaving mid-call moves the sound.
        devices = object : AudioDeviceCallback() {
            override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) = apply()
            override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) = apply()
        }.also { audio.registerAudioDeviceCallback(it, Handler(Looper.getMainLooper())) }
        apply()
    }

    fun setLoudspeaker(on: Boolean) {
        loudspeaker = on
        if (started) apply()
    }

    /** Hands the audio back: normal mode, no focus, no forced route, screen as usual. */
    @Suppress("DEPRECATION") // the pre-Android-12 routing calls
    fun stop() {
        if (!started) return
        started = false
        devices?.let { runCatching { audio.unregisterAudioDeviceCallback(it) } }
        devices = null
        releaseProximity()
        if (Build.VERSION.SDK_INT >= 31) runCatching { audio.clearCommunicationDevice() }
        else runCatching {
            audio.isSpeakerphoneOn = false
            audio.stopBluetoothSco()
        }
        focus?.let { runCatching { audio.abandonAudioFocusRequest(it) } }
        focus = null
        audio.mode = previousMode
    }

    @Suppress("DEPRECATION") // the pre-Android-12 routing calls
    private fun apply() {
        val route = chooseRoute(loudspeaker, bluetoothConnected())
        current.value = route
        if (Build.VERSION.SDK_INT >= 31) {
            val wanted = audio.availableCommunicationDevices.firstOrNull { it.type in typesFor(route) }
            if (wanted != null) runCatching { audio.setCommunicationDevice(wanted) } else audio.clearCommunicationDevice()
        } else {
            audio.isSpeakerphoneOn = route == AudioRoute.Speaker
            if (route == AudioRoute.Bluetooth) runCatching { audio.startBluetoothSco() } else runCatching { audio.stopBluetoothSco() }
        }
        if (route == AudioRoute.Earpiece) holdProximity() else releaseProximity()
    }

    private fun bluetoothConnected(): Boolean =
        if (Build.VERSION.SDK_INT >= 31) audio.availableCommunicationDevices.any { it.type in BLUETOOTH_TYPES }
        else audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any { it.type in BLUETOOTH_TYPES }

    private fun typesFor(route: AudioRoute): Set<Int> = when (route) {
        AudioRoute.Speaker -> setOf(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER)
        AudioRoute.Earpiece -> setOf(AudioDeviceInfo.TYPE_BUILTIN_EARPIECE)
        AudioRoute.Bluetooth -> BLUETOOTH_TYPES
    }

    /** Next to the ear the screen goes dark by itself, so a cheek can't press buttons. */
    private fun holdProximity() {
        if (proximity?.isHeld == true) return
        if (!power.isWakeLockLevelSupported(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK)) return
        proximity = power.newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "iris:call-proximity")
            .also { runCatching { it.acquire(4 * 60 * 60 * 1000L) } }
    }

    private fun releaseProximity() {
        proximity?.let { if (it.isHeld) runCatching { it.release() } }
        proximity = null
    }

    private companion object {
        val BLUETOOTH_TYPES: Set<Int> = buildSet {
            add(AudioDeviceInfo.TYPE_BLUETOOTH_SCO)
            if (Build.VERSION.SDK_INT >= 31) add(AudioDeviceInfo.TYPE_BLE_HEADSET)
        }
    }
}

package dev.iris.app.data

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.os.IBinder
import dev.iris.app.R

/**
 * Keeps a voice call alive while the screen is off or another app is on top: Android only lets
 * an app record from the microphone in the background through a foreground service of the
 * "microphone" kind. It shows an ongoing "Call with Iris" notification with a timer and a
 * Hang up button. The listening and speaking themselves are done by [CallController].
 *
 * It must be started while the app is on screen (Android 14 and later only allow that), which
 * is when the call screen opens.
 */
class CallService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_HANG_UP) {
            Calls.active?.hangUp()
            stopSelf()
            return START_NOT_STICKY
        }
        val call = Calls.active
        if (call == null) {
            // Android started this service, but the call is gone: it still has to show a notification first.
            startForeground(NOTIFICATION_ID, idle(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
            stopSelf()
            return START_NOT_STICKY
        }
        startForeground(NOTIFICATION_ID, notification(call), ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        return START_NOT_STICKY
    }

    /** Swiped away from the recent apps: the call ends with the app. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        Calls.active?.hangUp()
        stopSelf()
    }

    private fun channel() {
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "Calls", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shown while you are on a voice call with an agent"
                setShowBadge(false)
            },
        )
    }

    private fun idle(): Notification {
        channel()
        return Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_notify).setContentTitle("I.R.I.S. call ended").build()
    }

    private fun notification(call: CallController): Notification {
        channel()
        val hangUp = PendingIntent.getService(
            this, 0, Intent(this, CallService::class.java).setAction(ACTION_HANG_UP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notify)
            .setColor(0xFF8B5CF6.toInt())
            .setCategory(Notification.CATEGORY_CALL)
            .setContentTitle("Call with ${call.agent.name}")
            .setContentText("Tap to return to the call")
            // The system draws the running time itself: no updates needed.
            .setUsesChronometer(true)
            .setWhen(call.startedAt)
            .setShowWhen(true)
            .setOngoing(true)
            .setContentIntent(Push.openApp(this, call.agent.id))
            .addAction(Notification.Action.Builder(Icon.createWithResource(this, R.drawable.ic_notify), "Hang up", hangUp).build())
            .build()
    }

    companion object {
        private const val CHANNEL = "call"
        private const val NOTIFICATION_ID = 2
        const val ACTION_HANG_UP = "dev.iris.app.HANG_UP"

        /** Starts the foreground service for the call (call it while the app is on screen). */
        fun start(context: Context): Boolean =
            runCatching { context.startForegroundService(Intent(context, CallService::class.java)) }.isSuccess
    }
}

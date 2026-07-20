package org.xinutec.life

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat

/**
 * Fires when a scheduled reminder's alarm goes off (see [MainActivity.ReminderBridge])
 * and posts the notification. It's a manifest-declared receiver, not one registered in
 * code, so it still runs when the app process is gone — which is the whole point: the
 * reminder must reach you at 9am whether or not the app is open.
 *
 * The web app owns the "when" and the copy; this class only turns one alarm into one
 * notification, and routes a tap back into the app at the reminder's deep-link URL.
 */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra(EXTRA_ID) ?: return
        val title = intent.getStringExtra(EXTRA_TITLE) ?: context.getString(R.string.app_name)
        val body = intent.getStringExtra(EXTRA_BODY).orEmpty()
        val url = intent.getStringExtra(EXTRA_URL)

        ensureChannel(context)

        // Tapping the notification opens the app at the reminder's target route.
        // SINGLE_TOP so an already-running app is reused (onNewIntent) rather than
        // stacking a second Activity; NEW_TASK because we launch from a receiver.
        val tap =
            Intent(context, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
                if (url != null) putExtra(MainActivity.EXTRA_OPEN_URL, url)
            }
        val contentIntent =
            PendingIntent.getActivity(
                context,
                id.hashCode(),
                tap,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )

        val notification =
            NotificationCompat
                .Builder(context, CHANNEL_ID)
                .setSmallIcon(R.mipmap.ic_launcher)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .setContentIntent(contentIntent)
                .setAutoCancel(true)
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .build()

        // POST_NOTIFICATIONS (Android 13+) may be denied; notify() is then a silent
        // no-op, which is the right degradation — nothing to do from a receiver.
        context
            .getSystemService(NotificationManager::class.java)
            .notify(id.hashCode(), notification)
    }

    companion object {
        const val CHANNEL_ID = "reminders"
        const val EXTRA_ID = "reminder_id"
        const val EXTRA_TITLE = "reminder_title"
        const val EXTRA_BODY = "reminder_body"
        const val EXTRA_URL = "reminder_url"

        /** Create the reminders notification channel once; a no-op if it exists. */
        fun ensureChannel(context: Context) {
            val mgr = context.getSystemService(NotificationManager::class.java)
            if (mgr.getNotificationChannel(CHANNEL_ID) != null) return
            mgr.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID,
                    "Reminders",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ).apply {
                    description = "Scheduled nudges you set up in the app."
                },
            )
        }
    }
}

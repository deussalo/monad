package help.techmate.cogs

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log

/**
 * Keeps the machine sounding while the app is out of sight: a media-playback foreground service with a notification and
 * a media session (lock screen, headset buttons). It exists only from leaving the app while the machine plays until the
 * app comes back or the sound is stopped. The sound itself stays in Rust; this only says play, pause and stop.
 */
class PlaybackService : Service() {
    private var session: MediaSession? = null
    private var wake: PowerManager.WakeLock? = null
    private var focus: AudioFocusRequest? = null
    private var noisy: BroadcastReceiver? = null
    private var playing = true
    private var pausedByFocus = false

    private val audio get() = getSystemService(AUDIO_SERVICE) as AudioManager

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        running = true
        instance = this
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Playback", NotificationManager.IMPORTANCE_LOW).apply { setShowBadge(false) })
        session = MediaSession(this, "cogs").apply {
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() = setPlaying(true)
                override fun onPause() = setPlaying(false)
                override fun onStop() = stopAll()
            })
            isActive = true
        }
        wake = (getSystemService(POWER_SERVICE) as PowerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cogs:playback").apply { setReferenceCounted(false) }
        noisy = object : BroadcastReceiver() { override fun onReceive(c: Context, i: Intent) = setPlaying(false) }
        val filter = IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY)
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(noisy, filter, RECEIVER_NOT_EXPORTED) else registerReceiver(noisy, filter)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // first, whatever was asked: a service started this way must go to the foreground
        show()
        when (intent?.action) {
            ACTION_LEFT -> setPlaying(true, tell = false)
            ACTION_PLAY -> setPlaying(true)
            ACTION_PAUSE -> setPlaying(false)
            ACTION_STOP -> stopAll()
            ACTION_BACK -> finish()
        }
        return START_NOT_STICKY
    }

    /** The app was swiped away from recents: everything stops. */
    override fun onTaskRemoved(rootIntent: Intent?) { stopAll() }

    private fun setPlaying(p: Boolean, tell: Boolean = true) {
        pausedByFocus = false
        val want = p && takeFocus()
        if (p && !want) Log.i(TAG, "audio focus refused: paused")
        // leaving the app the machine is already playing: it only needs telling if it must stop
        if (tell || !want) Native.control(if (want) CONTROL_PLAY else CONTROL_PAUSE)
        playing = want
        wake?.let { if (want) it.acquire() else if (it.isHeld) it.release() }
        show()
    }

    private fun stopAll() { Native.control(CONTROL_STOP); finish() }

    private fun finish() {
        focus?.let { audio.abandonAudioFocusRequest(it) }; focus = null
        wake?.let { if (it.isHeld) it.release() }
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun takeFocus(): Boolean {
        val req = focus ?: AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setWillPauseWhenDucked(false)
            .setOnAudioFocusChangeListener { change ->
                when (change) {
                    AudioManager.AUDIOFOCUS_LOSS -> setPlaying(false)
                    AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> if (playing) { setPlaying(false); pausedByFocus = true }
                    AudioManager.AUDIOFOCUS_GAIN -> if (pausedByFocus) setPlaying(true)
                }
            }.build().also { focus = it }
        return audio.requestAudioFocus(req) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }

    private fun pending(action: String, code: Int) = PendingIntent.getService(this, code, Intent(this, PlaybackService::class.java).setAction(action),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    private fun show() {
        val s = session ?: return
        s.setPlaybackState(PlaybackState.Builder()
            .setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP)
            .setState(if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED, PlaybackState.PLAYBACK_POSITION_UNKNOWN, if (playing) 1f else 0f).build())
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val toggle = if (playing) Notification.Action.Builder(android.graphics.drawable.Icon.createWithResource(this, android.R.drawable.ic_media_pause), "Pause", pending(ACTION_PAUSE, 1))
            else Notification.Action.Builder(android.graphics.drawable.Icon.createWithResource(this, android.R.drawable.ic_media_play), "Play", pending(ACTION_PLAY, 2))
        val stop = Notification.Action.Builder(android.graphics.drawable.Icon.createWithResource(this, android.R.drawable.ic_menu_close_clear_cancel), "Stop", pending(ACTION_STOP, 3))
        val n = Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle("Cogs")
            .setContentText(if (playing) "Playing" else "Paused")
            .setContentIntent(open)
            .setDeleteIntent(pending(ACTION_STOP, 4))
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setOngoing(playing)
            .addAction(toggle.build())
            .addAction(stop.build())
            .setStyle(Notification.MediaStyle().setMediaSession(s.sessionToken).setShowActionsInCompactView(0, 1))
            .build()
        if (Build.VERSION.SDK_INT >= 29) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK) else startForeground(ID, n)
    }

    override fun onDestroy() {
        running = false
        if (instance === this) instance = null
        noisy?.let { unregisterReceiver(it) }
        session?.release()
        wake?.let { if (it.isHeld) it.release() }
        super.onDestroy()
    }

    companion object {
        private const val TAG = "cogs"
        private const val CHANNEL = "playback"
        private const val ID = 1
        private const val ACTION_LEFT = "help.techmate.cogs.LEFT"
        private const val ACTION_BACK = "help.techmate.cogs.BACK"
        private const val ACTION_PLAY = "help.techmate.cogs.PLAY"
        private const val ACTION_PAUSE = "help.techmate.cogs.PAUSE"
        private const val ACTION_STOP = "help.techmate.cogs.STOP"
        private const val CONTROL_PLAY = 1
        private const val CONTROL_PAUSE = 2
        private const val CONTROL_STOP = 3
        @Volatile private var running = false
        private var instance: PlaybackService? = null

        /** The activity is gone, and the sound engine with it: nothing is left for the service to keep. Main thread. */
        fun activityGone() { instance?.stopAll() }

        /** The app is leaving the screen: if the machine plays, keep it playing. Called while it is still visible. */
        fun left(c: Context) {
            if (!Native.playing()) return
            try { c.startForegroundService(Intent(c, PlaybackService::class.java).setAction(ACTION_LEFT)); running = true }
            catch (e: Exception) { Log.w(TAG, "could not start the playback service: $e") }
        }
        /** The app is back in front. */
        fun back(c: Context) {
            if (running) try { c.startService(Intent(c, PlaybackService::class.java).setAction(ACTION_BACK)) } catch (e: Exception) { Log.w(TAG, "playback service: $e") }
        }
    }
}

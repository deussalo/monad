package help.techmate.cogs

import android.Manifest
import android.app.AlertDialog
import android.app.NativeActivity
import android.bluetooth.BluetoothAdapter
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.widget.ArrayAdapter
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import kotlin.math.roundToInt

/**
 * The window, input, drawing and sound are Rust's (NativeActivity runs android_main in libcogs.so). This class only
 * does what needs Java: Bluetooth permission, finding keyboards, and opening MIDI devices; their messages go to
 * Rust through [Native.midi]. Rust calls [connectMidi] when the Connect button is tapped.
 */
class MainActivity : NativeActivity(), MidiHub.Listener {
    private lateinit var hub: MidiHub
    private var picker = false

    override fun onCreate(savedInstanceState: Bundle?) {
        System.loadLibrary("cogs")
        super.onCreate(savedInstanceState)
        window.decorView.setOnApplyWindowInsetsListener { v, ins ->
            if (Build.VERSION.SDK_INT >= 30) {
                val i = ins.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                Native.insets(i.top, i.bottom)
            } else {
                @Suppress("DEPRECATION")
                Native.insets(ins.systemWindowInsetTop, ins.systemWindowInsetBottom)
            }
            v.onApplyWindowInsets(ins)
        }
        hub = MidiHub(this, { NativeSink() }, this)
        hub.start()
        if (hub.savedAddress != null && hasBtPermissions() && hub.bluetoothOn) hub.scan(auto = true)
    }

    override fun onDestroy() {
        hub.stop()
        PlaybackService.activityGone()
        // The native side cannot outlive its activity: super.onDestroy would wait on it forever, and the playback service
        // would keep that frozen process alive, still sounding, with the app unable to open again. End the process here.
        android.os.Process.killProcess(android.os.Process.myPid())
        super.onDestroy()
    }
    override fun onResume() { super.onResume(); picker = false; Native.lifecycle(1); PlaybackService.back(this) }
    override fun onPause() {
        Native.lifecycle(2)
        // leaving, not just a file picker of ours in front: the sound goes on under a foreground service
        if (!picker && !isFinishing) PlaybackService.left(this)
        super.onPause()
    }
    @Deprecated("Deprecated in Java")
    override fun startActivityForResult(intent: Intent?, requestCode: Int) { picker = true; super.startActivityForResult(intent, requestCode) }
    override fun onStart() { super.onStart(); Native.lifecycle(3) }
    override fun onStop() { Native.lifecycle(4); super.onStop() }

    /** Called from Rust, on its thread. */
    @Suppress("unused")
    fun connectMidi() = runOnUiThread { connectFlow() }

    /** Called from Rust: Back with nothing left to close. */
    @Suppress("unused")
    fun leaveApp() = runOnUiThread { moveTaskToBack(true) }

    /** Called from Rust: pick a .cogs.json to open; its text goes back through Native.fileText. */
    @Suppress("unused")
    fun openFile() = runOnUiThread {
        val i = Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*")
        try { startActivityForResult(i, 10) } catch (e: Exception) { Native.fileText(null, "no file picker: ${e.message}") }
    }

    private var pendingSave: String? = null
    /** Called from Rust: ask where to save this text, then write it. */
    @Suppress("unused")
    fun saveFile(name: String, text: String) = runOnUiThread {
        pendingSave = text
        val i = Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("application/json").putExtra(Intent.EXTRA_TITLE, name)
        try { startActivityForResult(i, 11) } catch (e: Exception) { Toast.makeText(this, "no file picker", Toast.LENGTH_SHORT).show() }
    }

    override fun onStatus(text: String, connected: Boolean) = Native.status(text, connected)

    private var scanAdapter: ArrayAdapter<String>? = null
    private var scanItems: List<MidiHub.Found> = emptyList()
    private var scanDialog: AlertDialog? = null
    private var scanTitle: TextView? = null

    override fun onFound(found: List<MidiHub.Found>) {
        scanItems = found
        scanAdapter?.let { a -> a.clear(); a.addAll(found.map { (if (it.isMidi) "♪  " else "     ") + it.name + if (it.isMidi) "   · Bluetooth MIDI" else "" }) }
    }

    override fun onScanning(scanning: Boolean) {
        scanTitle?.text = if (scanning) "Looking for keyboards…" else if (scanItems.isEmpty()) "Nothing found. Is the keyboard in Bluetooth mode?" else "Tap your keyboard"
    }

    private fun btPermissions(): Array<String> =
        if (Build.VERSION.SDK_INT >= 31) arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
        else arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)

    private fun hasBtPermissions() = btPermissions().all { checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    @android.annotation.SuppressLint("MissingPermission") // checked by hasBtPermissions() first
    private fun connectFlow() {
        if (!hub.midiSupported) { toast("This phone has no MIDI support"); return }
        if (!hub.hasBluetooth) { toast("This phone has no Bluetooth"); return }
        if (!hasBtPermissions()) { requestPermissions(btPermissions(), 1); return }
        if (!hub.bluetoothOn) {
            @Suppress("DEPRECATION")
            runCatching { startActivityForResult(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE), 2) }.onFailure { toast("Turn Bluetooth on, then tap Connect") }
            return
        }
        if (Build.VERSION.SDK_INT < 31 && !locationOn()) {
            AlertDialog.Builder(this).setTitle("Location is off")
                .setMessage("Android ${Build.VERSION.RELEASE} only finds Bluetooth keyboards with Location turned on. Nothing is tracked.")
                .setPositiveButton("Open settings") { _, _ -> startActivity(Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS)) }
                .setNegativeButton("Cancel", null).show()
            return
        }
        showScanDialog()
    }

    private fun locationOn(): Boolean {
        val lm = getSystemService(Context.LOCATION_SERVICE) as LocationManager
        return if (Build.VERSION.SDK_INT >= 28) lm.isLocationEnabled
        else lm.isProviderEnabled(LocationManager.GPS_PROVIDER) || lm.isProviderEnabled(LocationManager.NETWORK_PROVIDER)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        if (requestCode == 1) { if (hasBtPermissions()) connectFlow() else toast("Bluetooth permission is needed to find the keyboard") }
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode == 2 && hub.bluetoothOn) connectFlow()
        if (requestCode == 10) {
            val uri = data?.data
            if (resultCode != RESULT_OK || uri == null) { Native.fileText(null, null); return }
            try {
                val text = contentResolver.openInputStream(uri)!!.use { it.readBytes().toString(Charsets.UTF_8) }
                Native.fileText(text, null)
            } catch (e: Exception) { Native.fileText(null, "could not read the file: ${e.message}") }
        }
        if (requestCode == 11) {
            val uri = data?.data; val text = pendingSave; pendingSave = null
            if (resultCode != RESULT_OK || uri == null || text == null) return
            try {
                contentResolver.openOutputStream(uri, "wt")!!.use { it.write(text.toByteArray(Charsets.UTF_8)) }
                Toast.makeText(this, "saved", Toast.LENGTH_SHORT).show()
            } catch (e: Exception) { Toast.makeText(this, "not saved: ${e.message}", Toast.LENGTH_LONG).show() }
        }
    }

    private fun showScanDialog() {
        val adapter = object : ArrayAdapter<String>(this, android.R.layout.simple_list_item_1, ArrayList()) {
            override fun getView(position: Int, convertView: View?, parent: ViewGroup): View =
                (super.getView(position, convertView, parent) as TextView).apply { textSize = 17f }
        }
        scanAdapter = adapter
        val list = ListView(this).apply { this.adapter = adapter; minimumHeight = dp(200) }
        val title = TextView(this).apply { textSize = 18f; setPadding(dp(20), dp(18), dp(20), dp(6)); text = "Looking for keyboards…" }
        scanTitle = title
        val hint = TextView(this).apply {
            textSize = 13f; setPadding(dp(20), 0, dp(20), dp(8))
            text = "Switch the keyboard on in Bluetooth mode. If it was paired to another phone or computer, unpair it there first. USB keyboards connect by themselves."
        }
        val box = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; addView(title); addView(hint); addView(list) }
        list.setOnItemClickListener { _, _, pos, _ -> scanItems.getOrNull(pos)?.let { hub.connect(it.device, it.name) }; scanDialog?.dismiss() }
        val b = AlertDialog.Builder(this, android.R.style.Theme_Material_Dialog).setView(box).setNegativeButton("Close", null)
        if (hub.savedName != null) b.setNeutralButton("Forget ${hub.savedName}") { _, _ -> hub.forget() }
        scanDialog = b.create().apply {
            setOnDismissListener { scanAdapter = null; scanTitle = null; scanDialog = null; if (!hub.isConnecting) { hub.stopScan(); if (hub.connectedCount == 0) hub.scan(auto = true) } }
            show()
        }
        hub.stopScan()
        hub.scan()
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).roundToInt()
    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_LONG).show()
}

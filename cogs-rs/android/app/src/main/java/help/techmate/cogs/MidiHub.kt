package help.techmate.cogs

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.media.midi.MidiDevice
import android.media.midi.MidiDeviceInfo
import android.media.midi.MidiManager
import android.media.midi.MidiOutputPort
import android.media.midi.MidiReceiver
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.util.Log
import java.util.UUID

/**
 * Finds MIDI keyboards and feeds them to a [MidiSink].
 *
 * Bluetooth LE MIDI (KORG microKEY Air / nanoKEY Air and other BLE-MIDI keyboards):
 * [scan] finds devices advertising the BLE-MIDI service, [connect] opens one through
 * MidiManager.openBluetoothDevice. The last keyboard is remembered and reconnected
 * automatically when it reappears.
 *
 * USB MIDI (the same keyboards over a cable) and devices other apps have opened: every
 * MIDI device with an output port that the system lists is opened automatically.
 */
@SuppressLint("MissingPermission") // the activity holds the permissions before calling scan/connect
class MidiHub(private val ctx: Context, private val sinkFactory: () -> MidiSink, private val listener: Listener) {

    interface Listener {
        fun onStatus(text: String, connected: Boolean)
        fun onFound(found: List<Found>)
        fun onScanning(scanning: Boolean)
    }

    class Found(val device: BluetoothDevice, val name: String, val isMidi: Boolean, var rssi: Int) {
        val address: String get() = device.address
        /** Ranks KORG keyboards and BLE-MIDI devices first. */
        val likely: Boolean get() = isMidi || looksLikeKeyboard(name)
    }

    private class Open(val device: MidiDevice, val ports: List<MidiOutputPort>, val name: String, val bluetooth: Boolean, val address: String?)

    companion object {
        val MIDI_SERVICE: UUID = UUID.fromString("03B80E5A-EDE8-4B33-A751-6CE34EC4C700")
        private const val TAG = "MidiHub"
        private const val PREFS = "midi"
        fun looksLikeKeyboard(name: String) = Regex("(?i)micro ?key|nano ?key|korg|midi|keys?|piano").containsMatchIn(name)
    }

    private val main = Handler(Looper.getMainLooper())
    private val mm = ctx.getSystemService(Context.MIDI_SERVICE) as MidiManager?
    private val bt = (ctx.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager?)?.adapter
    private val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val open = LinkedHashMap<Int, Open>()
    private val found = LinkedHashMap<String, Found>()
    private var scanning = false
    private var connecting: String? = null
    private var stopped = false

    val midiSupported get() = mm != null
    val bluetoothOn get() = bt?.isEnabled == true
    val hasBluetooth get() = bt != null
    var savedAddress: String?
        get() = prefs.getString("address", null)
        private set(v) { prefs.edit().putString("address", v).apply() }
    val savedName: String? get() = prefs.getString("name", null)

    private val deviceCallback = object : MidiManager.DeviceCallback() {
        override fun onDeviceAdded(info: MidiDeviceInfo) {
            // Bluetooth devices we opened ourselves arrive through openBluetoothDevice.
            if (info.type != MidiDeviceInfo.TYPE_BLUETOOTH && info.outputPortCount > 0) openInfo(info)
        }
        override fun onDeviceRemoved(info: MidiDeviceInfo) {
            val o = open.remove(info.id) ?: return
            closeQuietly(o)
            Log.i(TAG, "removed ${o.name}")
            status()
            if (o.bluetooth && !stopped) { listener.onStatus("${o.name} disconnected · waiting for it to come back", false); scan(auto = true) }
        }
    }

    /** Opens what the system already has and starts listening for new devices. */
    fun start() {
        val mm = mm ?: run { listener.onStatus("This phone has no MIDI support", false); return }
        stopped = false
        mm.registerDeviceCallback(deviceCallback, main)
        @Suppress("DEPRECATION")
        for (info in mm.devices) if (info.outputPortCount > 0) openInfo(info)
        status()
    }

    fun stop() {
        stopped = true
        stopScan()
        mm?.unregisterDeviceCallback(deviceCallback)
        for (o in open.values) closeQuietly(o)
        open.clear()
    }

    val connectedCount get() = open.size
    val isConnecting get() = connecting != null

    private fun nameOf(info: MidiDeviceInfo): String {
        val p = info.properties
        return p.getString(MidiDeviceInfo.PROPERTY_NAME) ?: p.getString(MidiDeviceInfo.PROPERTY_PRODUCT) ?: "MIDI device"
    }

    private fun openInfo(info: MidiDeviceInfo) {
        if (open.containsKey(info.id)) return
        mm?.openDevice(info, { dev -> if (dev != null) attach(dev, null) }, main)
    }

    private fun attach(dev: MidiDevice, address: String?) {
        val info = dev.info
        if (open.containsKey(info.id)) { dev.close(); return }
        val ports = (0 until info.outputPortCount).mapNotNull { i ->
            dev.openOutputPort(i)?.also { port ->
                val parser = MidiParser(sinkFactory())
                port.connect(object : MidiReceiver() {
                    override fun onSend(msg: ByteArray, offset: Int, count: Int, timestamp: Long) = parser.feed(msg, offset, count)
                })
            }
        }
        if (ports.isEmpty()) { dev.close(); return }
        val bluetooth = info.type == MidiDeviceInfo.TYPE_BLUETOOTH
        val name = nameOf(info)
        open[info.id] = Open(dev, ports, name, bluetooth, address)
        Log.i(TAG, "opened $name (${ports.size} port)")
        if (bluetooth && address != null) { savedAddress = address; prefs.edit().putString("name", name).apply(); stopScan() }
        status()
    }

    private fun closeQuietly(o: Open) {
        for (p in o.ports) runCatching { p.close() }
        runCatching { o.device.close() }
    }

    private fun status() {
        if (open.isEmpty()) {
            val saved = savedName
            listener.onStatus(if (saved != null) "Not connected · $saved will reconnect when switched on" else "No keyboard · tap Connect", false)
        } else listener.onStatus("Playing from " + open.values.joinToString(", ") { it.name }, true)
    }

    // ---- Bluetooth LE ----

    private val scanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, r: ScanResult) = seen(r)
        override fun onBatchScanResults(results: MutableList<ScanResult>) { for (r in results) seen(r) }
        override fun onScanFailed(errorCode: Int) {
            scanning = false; listener.onScanning(false)
            listener.onStatus("Bluetooth scan failed (error $errorCode). Turn Bluetooth off and on, then retry.", open.isNotEmpty())
        }
    }

    private var autoScan = false

    private fun seen(r: ScanResult) {
        val rec = r.scanRecord
        val isMidi = rec?.serviceUuids?.contains(ParcelUuid(MIDI_SERVICE)) == true
        val name = rec?.deviceName ?: runCatching { r.device.name }.getOrNull() ?: if (isMidi) "Bluetooth MIDI device" else return
        val f = found[r.device.address]
        if (f == null) found[r.device.address] = Found(r.device, name, isMidi, r.rssi) else f.rssi = r.rssi
        val wanted = r.device.address == savedAddress
        if (wanted && connecting == null && open.values.none { it.address == r.device.address }) { connect(r.device, name); return }
        if (!autoScan) listener.onFound(found.values.filter { it.likely || it.name.isNotBlank() }.sortedWith(compareBy({ !it.likely }, { -it.rssi })))
    }

    /** auto: a quiet background scan for the remembered keyboard; it keeps going until it appears. */
    fun scan(auto: Boolean = false) {
        val adapter = bt ?: run { listener.onStatus("This phone has no Bluetooth", open.isNotEmpty()); return }
        if (!adapter.isEnabled) { listener.onStatus("Bluetooth is off", open.isNotEmpty()); return }
        if (auto && savedAddress == null) return
        val scanner = adapter.bluetoothLeScanner ?: return
        if (scanning) { if (!auto) { autoScan = false; listener.onScanning(true) }; return }
        autoScan = auto
        found.clear()
        val settings = ScanSettings.Builder().setScanMode(if (auto) ScanSettings.SCAN_MODE_BALANCED else ScanSettings.SCAN_MODE_LOW_LATENCY).build()
        runCatching { scanner.startScan(null, settings, scanCallback) }.onFailure {
            listener.onStatus("Can't scan: ${it.message}", open.isNotEmpty()); return
        }
        scanning = true
        if (!auto) listener.onScanning(true)
        // Android throttles apps that scan too long; stop a manual scan after 25 s, a background one after 5 min.
        main.removeCallbacks(scanTimeout)
        main.postDelayed(scanTimeout, if (auto) 300_000L else 25_000L)
    }

    private val scanTimeout = Runnable { stopScan() }

    fun stopScan() {
        main.removeCallbacks(scanTimeout)
        if (!scanning) return
        scanning = false
        runCatching { bt?.bluetoothLeScanner?.stopScan(scanCallback) }
        listener.onScanning(false)
    }

    fun connect(device: BluetoothDevice, name: String) {
        val mm = mm ?: return
        if (connecting != null) return
        stopScan()
        connecting = device.address
        listener.onStatus("Connecting to $name…", open.isNotEmpty())
        // If nothing comes back (keyboard switched off mid-connect), give up and allow a retry.
        val timeout = Runnable { if (connecting == device.address) { connecting = null; listener.onStatus("$name didn't answer. Check it's in Bluetooth mode and try again.", open.isNotEmpty()) } }
        main.postDelayed(timeout, 15_000)
        mm.openBluetoothDevice(device, { dev ->
            main.removeCallbacks(timeout)
            connecting = null
            if (dev == null) listener.onStatus("Couldn't open $name. Switch it off and on and try again.", open.isNotEmpty())
            else attach(dev, device.address)
        }, main)
    }

    fun forget() {
        prefs.edit().clear().apply()
        for (o in open.values.filter { it.bluetooth }) { closeQuietly(o); open.remove(o.device.info.id) }
        status()
    }
}

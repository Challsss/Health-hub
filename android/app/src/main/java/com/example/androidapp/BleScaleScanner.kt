package com.example.androidapp

import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.os.ParcelUuid
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue

class BleScaleScanner(private val context: Context) {
    private val queue = ConcurrentLinkedQueue<JSONObject>()
    private var scanning = false

    private val miBeaconUuid: ParcelUuid =
        ParcelUuid.fromString("0000fe95-0000-1000-8000-00805f9b34fb")

    private val callback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            handleResult(result)
        }

        override fun onBatchScanResults(results: MutableList<ScanResult>) {
            for (r in results) {
                handleResult(r)
            }
        }
    }

    private fun handleResult(result: ScanResult) {
        val record = result.scanRecord ?: return
        val serviceData = record.getServiceData(miBeaconUuid) ?: return
        val hex = serviceData.joinToString("") { "%02x".format(it) }
        val obj = JSONObject().apply {
            put("mac", result.device?.address ?: "00:00:00:00:00:00")
            put("rssi", result.rssi)
            put("data_hex", hex)
            put("time_ms", System.currentTimeMillis())
        }
        queue.add(obj)
        while (queue.size > 200) {
            queue.poll()
        }
    }

    @SuppressLint("MissingPermission")
    fun startScan() {
        if (scanning) return
        val bm = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager ?: return
        val scanner = bm.adapter?.bluetoothLeScanner ?: return
        val filter = ScanFilter.Builder()
            .setServiceData(miBeaconUuid, byteArrayOf(), byteArrayOf())
            .build()
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()
        try {
            scanner.startScan(listOf(filter), settings, callback)
            scanning = true
        } catch (_: Throwable) {
        }
    }

    @SuppressLint("MissingPermission")
    fun stopScan() {
        if (!scanning) return
        val bm = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
        val scanner = bm?.adapter?.bluetoothLeScanner
        try {
            scanner?.stopScan(callback)
        } catch (_: Throwable) {
        }
        scanning = false
    }

    fun pollEventsJson(): String {
        val arr = JSONArray()
        while (true) {
            val item = queue.poll() ?: break
            arr.put(item)
        }
        return arr.toString()
    }
}

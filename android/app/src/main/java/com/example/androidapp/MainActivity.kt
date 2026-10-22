package com.example.androidapp

import android.Manifest
import android.app.NativeActivity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.WindowInsets
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

class MainActivity : NativeActivity() {
    companion object {
        init {
            System.loadLibrary("androidapp")
        }
    }

    private lateinit var bleScanner: BleScaleScanner

    override fun onCreate(savedInstanceState: Bundle?) {
        bleScanner = BleScaleScanner(this)
        super.onCreate(savedInstanceState)
    }

    override fun onDestroy() {
        bleScanner.stopScan()
        super.onDestroy()
    }

    @Suppress("unused")
    fun requestPermissions() {
        runOnUiThread {
            val intent = Intent(this, PermissionsActivity::class.java)
            startActivity(intent)
        }
    }

    @Suppress("unused")
    fun getPermissionsStatusJson(): String {
        val bleGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.BLUETOOTH_SCAN
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.ACCESS_FINE_LOCATION
            ) == PackageManager.PERMISSION_GRANTED
        }
        val hcAvail = HealthBridge.isAvailable(this)
        val hcGranted = HealthBridge.hasPermissions(this)
        return JSONObject().apply {
            put("health_granted", hcGranted)
            put("ble_granted", bleGranted)
            put("hc_available", hcAvail)
        }.toString()
    }

    @Suppress("unused")
    fun getSafeInsetsJson(): String {
        val density = resources.displayMetrics.density.coerceAtLeast(1f)
        var topDp = 48f
        var bottomDp = 28f
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val insets = window?.decorView?.rootWindowInsets?.getInsetsIgnoringVisibility(
                WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout()
            )
            if (insets != null) {
                topDp = (insets.top / density).coerceAtLeast(44f)
                bottomDp = (insets.bottom / density).coerceAtLeast(24f)
            }
        }
        return JSONObject().apply {
            put("top_dp", topDp.toDouble())
            put("bottom_dp", bottomDp.toDouble())
        }.toString()
    }

    @Suppress("unused")
    fun getClipboardText(): String {
        val ref = AtomicReference("")
        val latch = CountDownLatch(1)
        runOnUiThread {
            try {
                val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                val item = cm?.primaryClip?.getItemAt(0)
                val text = item?.coerceToText(this)?.toString() ?: ""
                ref.set(text)
            } catch (_: Throwable) {
            } finally {
                latch.countDown()
            }
        }
        latch.await(1500, TimeUnit.MILLISECONDS)
        return ref.get()
    }

    @Suppress("unused")
    fun setClipboardText(text: String) {
        runOnUiThread {
            try {
                val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                cm?.setPrimaryClip(ClipData.newPlainText("Health Hub", text))
            } catch (_: Throwable) {
            }
        }
    }

    @Suppress("unused")
    fun readHealthJson(startMs: Long, endMs: Long): String {
        return HealthBridge.readAllJson(this, startMs, endMs)
    }

    @Suppress("unused")
    fun startScaleScan() {
        bleScanner.startScan()
    }

    @Suppress("unused")
    fun stopScaleScan() {
        bleScanner.stopScan()
    }

    @Suppress("unused")
    fun pollBleEventsJson(): String {
        return bleScanner.pollEventsJson()
    }
}

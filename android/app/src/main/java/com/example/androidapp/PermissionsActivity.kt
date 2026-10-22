package com.example.androidapp

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.PermissionController

class PermissionsActivity : ComponentActivity() {
    private lateinit var statusView: TextView

    private val hcLauncher = registerForActivityResult(
        PermissionController.createRequestPermissionResultContract()
    ) { _ ->
        updateStatusText()
    }

    private val bleLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { _ ->
        updateStatusText()
        if (HealthBridge.isAvailable(this) &&
            HealthBridge.grantedCount(this) < HealthBridge.PERMISSIONS.size
        ) {
            try {
                hcLauncher.launch(HealthBridge.PERMISSIONS)
            } catch (_: Throwable) {
                openHealthConnectSettings()
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        window.statusBarColor = Color.parseColor("#140E0A")
        window.navigationBarColor = Color.parseColor("#140E0A")

        val density = resources.displayMetrics.density
        fun dp(v: Int): Int = (v * density).toInt()

        val rootScroll = ScrollView(this).apply {
            setBackgroundColor(Color.parseColor("#140E0A"))
            isFillViewport = true
        }

        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(56), dp(24), dp(40))
        }

        val title = TextView(this).apply {
            text = "Permissions & Health Connect"
            textSize = 22f
            setTextColor(Color.parseColor("#FCF3EA"))
            setPadding(0, 0, 0, dp(12))
        }

        statusView = TextView(this).apply {
            textSize = 15f
            setTextColor(Color.parseColor("#F97316"))
            setPadding(dp(16), dp(16), dp(16), dp(16))
            background = GradientDrawable().apply {
                setColor(Color.parseColor("#221812"))
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), Color.parseColor("#523826"))
            }
        }

        val body = TextView(this).apply {
            text = """
                • Fitbit Air metrics (steps, distance, calories, exercise, heart rate, resting HR, HRV, sleep) are read via Android Health Connect.
                • Xiaomi Scale S400 measurements are received via Bluetooth Low Energy (BLE 0xFE95 broadcasts).
                • All data stays 100% locally on your phone.
            """.trimIndent()
            textSize = 14f
            setTextColor(Color.parseColor("#BC9C82"))
            setPadding(0, dp(16), 0, dp(24))
        }

        fun styledButton(label: String, bgHex: String, fgHex: String, onClick: () -> Unit): Button {
            return Button(this).apply {
                text = label
                isAllCaps = false
                textSize = 15f
                setTextColor(Color.parseColor(fgHex))
                background = GradientDrawable().apply {
                    setColor(Color.parseColor(bgHex))
                    cornerRadius = dp(14).toFloat()
                    setStroke(dp(1), Color.parseColor("#523826"))
                }
                val lp = LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT
                ).apply {
                    bottomMargin = dp(12)
                }
                layoutParams = lp
                setPadding(dp(16), dp(14), dp(16), dp(14))
                gravity = Gravity.CENTER
                setOnClickListener { onClick() }
            }
        }

        val btnRequest = styledButton(
            "Request Missing Permissions",
            "#F97316",
            "#140E0A"
        ) {
            requestAll()
        }

        val btnHealthConnect = styledButton(
            "Manage Health Connect Permissions",
            "#322319",
            "#FCF3EA"
        ) {
            openHealthConnectSettings()
        }

        val btnAppSettings = styledButton(
            "Open Android App Permissions",
            "#322319",
            "#FCF3EA"
        ) {
            openAppSettings()
        }

        val btnDone = styledButton(
            "← Back to Health Hub",
            "#221812",
            "#F97316"
        ) {
            finish()
        }

        layout.addView(title)
        layout.addView(statusView)
        layout.addView(body)
        layout.addView(btnRequest)
        layout.addView(btnHealthConnect)
        layout.addView(btnAppSettings)
        layout.addView(btnDone)

        rootScroll.addView(layout)
        setContentView(rootScroll)

        updateStatusText()

        val rationaleAction = intent?.action
        val isRationaleOnly =
            rationaleAction == "androidx.health.ACTION_SHOW_PERMISSIONS_RATIONALE" ||
                rationaleAction == "android.intent.action.VIEW_PERMISSION_USAGE"

        if (!isRationaleOnly && (!isBleGranted() || HealthBridge.grantedCount(this) < HealthBridge.PERMISSIONS.size)) {
            requestAll()
        }
    }

    override fun onResume() {
        super.onResume()
        updateStatusText()
    }

    private fun isBleGranted(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
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
    }

    private fun updateStatusText() {
        val bleOk = isBleGranted()
        val hcAvail = HealthBridge.isAvailable(this)
        val hcCount = HealthBridge.grantedCount(this)
        val hcTotal = HealthBridge.PERMISSIONS.size
        statusView.text = buildString {
            appendLine(
                "Bluetooth (Xiaomi S400): " + if (bleOk) "Granted ✓" else "Not Granted"
            )
            append(
                "Health Connect (Fitbit Air): " +
                    if (!hcAvail) "Not Available"
                    else if (hcCount > 0) "Granted ✓ ($hcCount/$hcTotal record types)"
                    else "Not Granted (0/$hcTotal)"
            )
        }
    }

    private fun requestAll() {
        val blePerms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
            )
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        bleLauncher.launch(blePerms)
    }

    private fun openHealthConnectSettings() {
        try {
            val intent = Intent("android.health.connect.action.MANAGE_HEALTH_PERMISSIONS").apply {
                putExtra(Intent.EXTRA_PACKAGE_NAME, packageName)
            }
            startActivity(intent)
            return
        } catch (_: Throwable) {
        }
        try {
            val intent = HealthConnectClient.getHealthConnectManageDataIntent(this)
            startActivity(intent)
        } catch (_: Throwable) {
            openAppSettings()
        }
    }

    private fun openAppSettings() {
        try {
            val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                data = Uri.fromParts("package", packageName, null)
            }
            startActivity(intent)
        } catch (_: Throwable) {
        }
    }
}

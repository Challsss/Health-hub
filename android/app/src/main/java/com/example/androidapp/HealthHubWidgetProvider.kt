package com.example.androidapp

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import org.json.JSONObject
import java.io.File
import java.text.NumberFormat
import java.time.LocalDate
import java.util.Locale
import kotlin.math.max
import kotlin.math.min

class HealthHubWidgetProvider : AppWidgetProvider() {

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (widgetId in appWidgetIds) {
            updateWidget(context, appWidgetManager, widgetId)
        }
    }

    companion object {
        fun updateAllWidgets(context: Context) {
            try {
                val mgr = AppWidgetManager.getInstance(context)
                val ids = mgr.getAppWidgetIds(
                    ComponentName(context, HealthHubWidgetProvider::class.java)
                )
                if (ids != null && ids.isNotEmpty()) {
                    for (id in ids) {
                        updateWidget(context, mgr, id)
                    }
                }
            } catch (_: Throwable) {
            }
        }

        private fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
        ) {
            val views = RemoteViews(context.packageName, R.layout.widget_health_hub)

            // One-click launch of MainActivity when tapping anywhere on the widget
            val launchIntent = Intent(context, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            val pendingIntent = PendingIntent.getActivity(
                context,
                0,
                launchIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            views.setOnClickPendingIntent(R.id.widget_root, pendingIntent)

            val summary = readStoreSummary(context)
            views.setTextViewText(R.id.widget_updated, summary.headerRight)
            views.setTextViewText(R.id.widget_steps_val, summary.stepsMain)
            views.setTextViewText(R.id.widget_fitbit_sub, summary.fitbitSub)
            views.setTextViewText(R.id.widget_weight_val, summary.weightMain)
            views.setTextViewText(R.id.widget_scale_sub, summary.scaleSub)

            appWidgetManager.updateAppWidget(appWidgetId, views)
        }

        private data class WidgetSummary(
            val headerRight: String,
            val stepsMain: String,
            val fitbitSub: String,
            val weightMain: String,
            val scaleSub: String
        )

        private fun readStoreSummary(context: Context): WidgetSummary {
            val file = File(context.filesDir, "health_hub_store.json")
            if (!file.exists()) {
                return WidgetSummary(
                    headerRight = "Tap to open",
                    stepsMain = "– steps",
                    fitbitSub = "Sleep – • RHR –",
                    weightMain = "– kg",
                    scaleSub = "Fat –% • Visceral –"
                )
            }

            return try {
                val root = JSONObject(file.readText())
                val nf = NumberFormat.getIntegerInstance(Locale.US)

                // 1. Latest Fitbit Air day (steps + resting HR)
                var latestDate = ""
                var stepsText = "– steps"
                var rhrText = "RHR –"
                val daysObj = root.optJSONObject("days")
                if (daysObj != null) {
                    val keys = daysObj.keys().asSequence().toList().sorted()
                    val bestKey = keys.lastOrNull { k ->
                        val d = daysObj.optJSONObject(k)
                        d != null && (!d.isNull("steps") || !d.isNull("resting_hr"))
                    } ?: keys.lastOrNull()

                    if (bestKey != null) {
                        latestDate = bestKey
                        val day = daysObj.optJSONObject(bestKey)
                        if (day != null) {
                            if (!day.isNull("steps")) {
                                val s = day.optLong("steps", 0L)
                                stepsText = "${nf.format(s)} steps"
                            }
                            if (!day.isNull("resting_hr")) {
                                val rhr = day.optDouble("resting_hr", 0.0)
                                if (rhr > 0) {
                                    rhrText = String.format(Locale.US, "RHR %.0f bpm", rhr)
                                }
                            } else if (!day.isNull("hrv_rmssd")) {
                                val hrv = day.optDouble("hrv_rmssd", 0.0)
                                if (hrv > 0) {
                                    rhrText = String.format(Locale.US, "HRV %.0f ms", hrv)
                                }
                            }
                        }
                    }
                }

                // 2. Latest Sleep session
                var sleepText = "Sleep –"
                val sleepArr = root.optJSONArray("sleep")
                if (sleepArr != null && sleepArr.length() > 0) {
                    val sObj = sleepArr.optJSONObject(sleepArr.length() - 1)
                    if (sObj != null) {
                        val startMs = sObj.optLong("start_ms", 0L)
                        val endMs = sObj.optLong("end_ms", 0L)
                        val inBedMin = max(0.0, (endMs - startMs) / 60000.0)
                        var awakeMin = 0.0
                        val stages = sObj.optJSONArray("stages")
                        if (stages != null && stages.length() > 0) {
                            for (i in 0 until stages.length()) {
                                val st = stages.optJSONObject(i) ?: continue
                                if (st.optString("kind") == "Awake") {
                                    val sm = st.optLong("start_ms", 0L)
                                    val em = st.optLong("end_ms", 0L)
                                    awakeMin += max(0.0, (em - sm) / 60000.0)
                                }
                            }
                        }
                        val asleepMin = max(0.0, inBedMin - awakeMin)
                        val hrs = asleepMin.toInt() / 60
                        val mins = asleepMin.toInt() % 60
                        sleepText = String.format(Locale.US, "Sleep %dh %02dm", hrs, mins)
                    }
                }

                // 3. Latest Xiaomi S400 Scale measurement
                var weightMain = "– kg"
                var scaleSub = "Fat –% • Visceral –"
                val scaleArr = root.optJSONArray("scale")
                if (scaleArr != null && scaleArr.length() > 0) {
                    val mObj = scaleArr.optJSONObject(scaleArr.length() - 1)
                    if (mObj != null) {
                        val w = mObj.optDouble("weight_kg", 0.0).toFloat()
                        if (w > 0f) {
                            weightMain = String.format(Locale.US, "%.1f kg", w)

                            val profile = root.optJSONObject("profile")
                            val heightCm = profile?.optDouble("height_cm", 175.0)?.toFloat() ?: 175.0f
                            val birthYear = profile?.optInt("birth_year", 1995) ?: 1995
                            val male = (profile?.optString("sex", "Male") ?: "Male") == "Male"
                            val age = max(10, LocalDate.now().year - birthYear).toFloat()

                            val z1 = if (!mObj.isNull("impedance_low")) {
                                mObj.optDouble("impedance_low", 0.0).toFloat()
                            } else 0f
                            val z2 = if (!mObj.isNull("impedance_high")) {
                                mObj.optDouble("impedance_high", 0.0).toFloat()
                            } else 0f

                            val valid1 = z1 in 100f..3000f
                            val valid2 = z2 in 100f..3000f

                            if (valid1 || valid2) {
                                val isDual = valid1 && valid2
                                val zLf = if (isDual) max(z1, z2) else if (valid1) z1 else z2
                                val zHf = if (isDual) min(z1, z2) else 0f
                                val zEff = if (isDual) 0.70f * zLf + 0.30f * zHf else zLf

                                var lbm = (heightCm * 9.058f / 100f) * (heightCm / 100f) +
                                    w * 0.32f + 12.226f - zEff * 0.0068f - age * 0.0542f
                                if (!male && isDual) {
                                    lbm *= 0.84f
                                }
                                lbm = min(lbm, w * 0.98f)

                                val bf = if (isDual) {
                                    val rawBf = ((w - lbm) / w) * 100f
                                    rawBf.coerceIn(if (male) 5f else 10f, 75f)
                                } else {
                                    val sub = if (!male && age <= 49f) 9.25f else if (!male) 7.25f else 0.8f
                                    val rawBf = (1f - ((lbm - sub) / w)) * 100f
                                    rawBf.coerceIn(3f, 75f)
                                }

                                val vf = if (isDual) {
                                    val hM = heightCm / 100f
                                    val fatMass = w * (bf / 100f)
                                    val fmi = fatMass / (hM * hM)
                                    (if (male) fmi + age * 0.05f else fmi * 0.85f + age * 0.05f)
                                        .coerceIn(1f, 50f)
                                } else {
                                    val sub = (heightCm * 0.0826f * heightCm) - (heightCm * 0.4f)
                                    (((w * 305f) / (sub + 48f)) - 2.9f + age * 0.15f).coerceIn(1f, 50f)
                                }

                                scaleSub = String.format(
                                    Locale.US,
                                    "%.1f%% fat • Visceral %.0f",
                                    bf,
                                    vf
                                )
                            } else {
                                val hM = heightCm / 100f
                                val bmi = w / (hM * hM)
                                scaleSub = String.format(Locale.US, "BMI %.1f", bmi)
                            }
                        }
                    }
                }

                val headerRight = if (latestDate.isNotEmpty()) {
                    "$latestDate • Open"
                } else {
                    "Tap to open"
                }

                WidgetSummary(
                    headerRight = headerRight,
                    stepsMain = stepsText,
                    fitbitSub = "$sleepText • $rhrText",
                    weightMain = weightMain,
                    scaleSub = scaleSub
                )
            } catch (_: Throwable) {
                WidgetSummary(
                    headerRight = "Tap to open",
                    stepsMain = "– steps",
                    fitbitSub = "Sleep – • RHR –",
                    weightMain = "– kg",
                    scaleSub = "Fat –% • Visceral –"
                )
            }
        }
    }
}

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
import java.text.SimpleDateFormat
import java.time.LocalDate
import java.util.Date
import java.util.Locale
import kotlin.math.max
import kotlin.math.min

/**
 * Widget 1 (4x2): Steps & Heart (Google Fitbit Air)
 */
class HealthHubWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (id in appWidgetIds) {
            WidgetUpdater.updateActivityWidget(context, appWidgetManager, id)
        }
    }

    companion object {
        fun updateAllWidgets(context: Context) {
            WidgetUpdater.updateAll(context)
        }
    }
}

/**
 * Widget 2 (4x2): Last Sleep (Google Fitbit Air)
 */
class SleepWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (id in appWidgetIds) {
            WidgetUpdater.updateSleepWidget(context, appWidgetManager, id)
        }
    }
}

/**
 * Widget 3 (4x2): Last Weigh-in (Xiaomi Scale S400)
 */
class ScaleWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        for (id in appWidgetIds) {
            WidgetUpdater.updateScaleWidget(context, appWidgetManager, id)
        }
    }
}

internal object WidgetUpdater {
    fun updateAll(context: Context) {
        try {
            val mgr = AppWidgetManager.getInstance(context)
            mgr.getAppWidgetIds(ComponentName(context, HealthHubWidgetProvider::class.java))
                ?.forEach { updateActivityWidget(context, mgr, it) }
            mgr.getAppWidgetIds(ComponentName(context, SleepWidgetProvider::class.java))
                ?.forEach { updateSleepWidget(context, mgr, it) }
            mgr.getAppWidgetIds(ComponentName(context, ScaleWidgetProvider::class.java))
                ?.forEach { updateScaleWidget(context, mgr, it) }
        } catch (_: Throwable) {
        }
    }

    private fun attachLaunchIntent(context: Context, views: RemoteViews, requestCode: Int) {
        val launchIntent = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pi = PendingIntent.getActivity(
            context,
            requestCode,
            launchIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        views.setOnClickPendingIntent(R.id.widget_root, pi)
    }

    private fun loadStore(context: Context): JSONObject? {
        return try {
            val file = File(context.filesDir, "health_hub_store.json")
            if (file.exists()) JSONObject(file.readText()) else null
        } catch (_: Throwable) {
            null
        }
    }

    // -----------------------------------------------------------------------
    // 1. Steps & Heart 4x2 Widget
    // -----------------------------------------------------------------------
    fun updateActivityWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 101)

        views.setTextViewText(R.id.widget_title, "♥ STEPS & HEART")
        views.setTextViewText(R.id.widget_left_label, "DAILY STEPS")
        views.setTextViewText(R.id.widget_right_label, "RESTING HR")

        val root = loadStore(context)
        val daysObj = root?.optJSONObject("days")
        if (daysObj == null) {
            views.setTextViewText(R.id.widget_updated, "Tap to sync")
            views.setTextViewText(R.id.widget_left_main, "0")
            views.setTextViewText(R.id.widget_left_sub, "0.0 km • 0 kcal")
            views.setTextViewText(R.id.widget_right_main, "– bpm")
            views.setTextViewText(R.id.widget_right_sub, "HRV – • Ex 0m")
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val nf = NumberFormat.getIntegerInstance(Locale.US)
        val keys = daysObj.keys().asSequence().toList().sorted()
        val bestKey = keys.lastOrNull { k ->
            val d = daysObj.optJSONObject(k)
            d != null && (!d.isNull("steps") || !d.isNull("resting_hr") || !d.isNull("hrv_rmssd"))
        } ?: keys.lastOrNull()

        if (bestKey == null) {
            views.setTextViewText(R.id.widget_updated, "Tap to sync")
            views.setTextViewText(R.id.widget_left_main, "0")
            views.setTextViewText(R.id.widget_left_sub, "0.0 km • 0 kcal")
            views.setTextViewText(R.id.widget_right_main, "– bpm")
            views.setTextViewText(R.id.widget_right_sub, "HRV – • Ex 0m")
        } else {
            val d = daysObj.optJSONObject(bestKey)
            val steps = if (d != null && !d.isNull("steps")) {
                nf.format(d.optLong("steps", 0L))
            } else "0"

            val distKm = if (d != null && !d.isNull("distance_m")) {
                String.format(Locale.US, "%.1f km", d.optDouble("distance_m", 0.0) / 1000.0)
            } else "0.0 km"

            val kcal = if (d != null && !d.isNull("active_kcal")) {
                String.format(Locale.US, "%.0f kcal", d.optDouble("active_kcal", 0.0))
            } else "0 kcal"

            // Search most recent day with resting_hr / hrv if today doesn't have it yet
            var rhrVal: Double? = null
            var hrvVal: Double? = null
            for (k in keys.asReversed()) {
                val dayItem = daysObj.optJSONObject(k) ?: continue
                if (rhrVal == null && !dayItem.isNull("resting_hr")) {
                    val v = dayItem.optDouble("resting_hr", 0.0)
                    if (v > 0) rhrVal = v
                }
                if (hrvVal == null && !dayItem.isNull("hrv_rmssd")) {
                    val v = dayItem.optDouble("hrv_rmssd", 0.0)
                    if (v > 0) hrvVal = v
                }
                if (rhrVal != null && hrvVal != null) break
            }

            val rhrMain = rhrVal?.let { String.format(Locale.US, "%.0f bpm", it) } ?: "– bpm"
            val hrvStr = hrvVal?.let { String.format(Locale.US, "HRV %.0f ms", it) } ?: "HRV –"
            val exStr = if (d != null && !d.isNull("exercise_min")) {
                String.format(Locale.US, "Ex %.0fm", d.optDouble("exercise_min", 0.0))
            } else "Ex 0m"

            views.setTextViewText(R.id.widget_updated, bestKey)
            views.setTextViewText(R.id.widget_left_main, steps)
            views.setTextViewText(R.id.widget_left_sub, "$distKm • $kcal")
            views.setTextViewText(R.id.widget_right_main, rhrMain)
            views.setTextViewText(R.id.widget_right_sub, "$hrvStr • $exStr")
        }

        mgr.updateAppWidget(widgetId, views)
    }

    // -----------------------------------------------------------------------
    // 2. Last Sleep 4x2 Widget
    // -----------------------------------------------------------------------
    fun updateSleepWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 102)

        views.setTextViewText(R.id.widget_title, "🌙 SLEEP & RECOVERY")
        views.setTextViewText(R.id.widget_left_label, "TIME ASLEEP")
        views.setTextViewText(R.id.widget_right_label, "DEEP SLEEP")

        val root = loadStore(context)
        val sleepArr = root?.optJSONArray("sleep")
        if (sleepArr == null || sleepArr.length() == 0) {
            views.setTextViewText(R.id.widget_updated, "Tap to sync")
            views.setTextViewText(R.id.widget_left_main, "–h –m")
            views.setTextViewText(R.id.widget_left_sub, "Bedtime –")
            views.setTextViewText(R.id.widget_right_main, "– min")
            views.setTextViewText(R.id.widget_right_sub, "REM – • Light –")
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val sObj = sleepArr.optJSONObject(sleepArr.length() - 1)
        if (sObj != null) {
            val dateStr = sObj.optString("date", "Last night")
            val startMs = sObj.optLong("start_ms", 0L)
            val endMs = sObj.optLong("end_ms", 0L)
            val inBedMin = max(0.0, (endMs - startMs) / 60000.0)
            val bedMin = sObj.optInt("bedtime_min", 0).let { ((it % 1440) + 1440) % 1440 }
            val bedClock = String.format(Locale.US, "%02d:%02d", bedMin / 60, bedMin % 60)

            var deepMin = 0.0
            var remMin = 0.0
            var lightMin = 0.0
            var awakeMin = 0.0
            val stages = sObj.optJSONArray("stages")
            if (stages != null && stages.length() > 0) {
                for (i in 0 until stages.length()) {
                    val st = stages.optJSONObject(i) ?: continue
                    val dur = max(0.0, (st.optLong("end_ms", 0L) - st.optLong("start_ms", 0L)) / 60000.0)
                    when (st.optString("kind")) {
                        "Deep" -> deepMin += dur
                        "Rem" -> remMin += dur
                        "Awake" -> awakeMin += dur
                        else -> lightMin += dur
                    }
                }
            } else {
                lightMin = inBedMin
            }

            val asleepMin = if (stages != null && stages.length() > 0) {
                deepMin + remMin + lightMin
            } else {
                inBedMin
            }

            val asleepStr = String.format(
                Locale.US,
                "%dh %02dm",
                asleepMin.toInt() / 60,
                asleepMin.toInt() % 60
            )
            val inBedStr = String.format(
                Locale.US,
                "%dh %02dm",
                inBedMin.toInt() / 60,
                inBedMin.toInt() % 60
            )
            val deepHero = if (deepMin >= 60.0) {
                String.format(Locale.US, "%dh %02dm", deepMin.toInt() / 60, deepMin.toInt() % 60)
            } else {
                String.format(Locale.US, "%.0f min", deepMin)
            }

            views.setTextViewText(R.id.widget_updated, dateStr)
            views.setTextViewText(R.id.widget_left_main, asleepStr)
            views.setTextViewText(R.id.widget_left_sub, "Bed $bedClock • Tot $inBedStr")
            views.setTextViewText(R.id.widget_right_main, deepHero)
            views.setTextViewText(
                R.id.widget_right_sub,
                String.format(Locale.US, "REM %.0fm • Light %.0fm", remMin, lightMin)
            )
        }

        mgr.updateAppWidget(widgetId, views)
    }

    // -----------------------------------------------------------------------
    // 3. Last Weigh-in (Xiaomi Scale S400) 4x2 Widget
    // -----------------------------------------------------------------------
    fun updateScaleWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 103)

        views.setTextViewText(R.id.widget_title, "⚖ XIAOMI SCALE S400")
        views.setTextViewText(R.id.widget_left_label, "WEIGHT")
        views.setTextViewText(R.id.widget_right_label, "BODY FAT")

        val root = loadStore(context)
        val scaleArr = root?.optJSONArray("scale")
        if (scaleArr == null || scaleArr.length() == 0) {
            views.setTextViewText(R.id.widget_updated, "Tap to scan")
            views.setTextViewText(R.id.widget_left_main, "– kg")
            views.setTextViewText(R.id.widget_left_sub, "BMI – • Muscle –")
            views.setTextViewText(R.id.widget_right_main, "– %")
            views.setTextViewText(R.id.widget_right_sub, "Visceral – • Water –%")
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val mObj = scaleArr.optJSONObject(scaleArr.length() - 1)
        if (mObj != null) {
            val w = mObj.optDouble("weight_kg", 0.0).toFloat()
            val timeMs = mObj.optLong("time_ms", 0L)
            val dateStr = if (timeMs > 0L) {
                SimpleDateFormat("MMM d, HH:mm", Locale.US).format(Date(timeMs))
            } else "Latest"

            val profile = root.optJSONObject("profile")
            val heightCm = profile?.optDouble("height_cm", 175.0)?.toFloat() ?: 175.0f
            val birthYear = profile?.optInt("birth_year", 1995) ?: 1995
            val male = (profile?.optString("sex", "Male") ?: "Male") == "Male"
            val age = max(10, LocalDate.now().year - birthYear).toFloat()
            val hM = heightCm / 100f
            val bmi = w / (hM * hM)

            val z1 = if (!mObj.isNull("impedance_low")) {
                mObj.optDouble("impedance_low", 0.0).toFloat()
            } else 0f
            val z2 = if (!mObj.isNull("impedance_high")) {
                mObj.optDouble("impedance_high", 0.0).toFloat()
            } else 0f
            val valid1 = z1 in 100f..3000f
            val valid2 = z2 in 100f..3000f

            views.setTextViewText(R.id.widget_updated, dateStr)
            views.setTextViewText(R.id.widget_left_main, String.format(Locale.US, "%.1f kg", w))

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
                    (((w - lbm) / w) * 100f).coerceIn(if (male) 5f else 10f, 75f)
                } else {
                    val sub = if (!male && age <= 49f) 9.25f else if (!male) 7.25f else 0.8f
                    ((1f - ((lbm - sub) / w)) * 100f).coerceIn(3f, 75f)
                }

                val fatMass = w * (bf / 100f)
                val baseBone = if (male) 0.18016894f else 0.245691014f
                var bone = (baseBone - lbm * 0.05158f) * -1f
                bone += if (bone > 2.2f) 0.1f else -0.1f
                bone = bone.coerceIn(0.5f, 8.0f)
                val muscle = max(0f, (w - fatMass) - bone)
                val water = if (isDual) {
                    ((100f - bf) * 0.73f).coerceIn(35f, 75f)
                } else {
                    val w0 = (100f - bf) * 0.7f
                    (w0 * if (w0 <= 50f) 1.02f else 0.98f).coerceIn(35f, 75f)
                }
                val vf = if (isDual) {
                    val fmi = fatMass / (hM * hM)
                    (if (male) fmi + age * 0.05f else fmi * 0.85f + age * 0.05f).coerceIn(1f, 50f)
                } else {
                    val sub = (heightCm * 0.0826f * heightCm) - (heightCm * 0.4f)
                    (((w * 305f) / (sub + 48f)) - 2.9f + age * 0.15f).coerceIn(1f, 50f)
                }

                views.setTextViewText(
                    R.id.widget_left_sub,
                    String.format(Locale.US, "BMI %.1f • Muscle %.1f kg", bmi, muscle)
                )
                views.setTextViewText(
                    R.id.widget_right_main,
                    String.format(Locale.US, "%.1f %%", bf)
                )
                views.setTextViewText(
                    R.id.widget_right_sub,
                    String.format(Locale.US, "Visceral %.0f • Water %.1f%%", vf, water)
                )
            } else {
                views.setTextViewText(
                    R.id.widget_left_sub,
                    String.format(Locale.US, "BMI %.1f", bmi)
                )
                views.setTextViewText(R.id.widget_right_main, "– %")
                views.setTextViewText(R.id.widget_right_sub, "Step barefoot for BIA")
            }
        }

        mgr.updateAppWidget(widgetId, views)
    }
}

package com.example.androidapp

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import android.widget.RemoteViews
import org.json.JSONObject
import java.io.File
import java.text.NumberFormat
import java.time.LocalDate
import java.util.Locale
import kotlin.math.max
import kotlin.math.min

/**
 * Widget 1 (4x2): Steps & Heart (Google Fitbit Air) with 7-day bar chart + HR sparkline
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
 * Widget 2 (4x2): Last Sleep (Google Fitbit Air) with 7-night bar chart + sleep stage breakdown bars
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
 * Widget 3 (4x2): Last Weigh-in (Xiaomi Scale S400) with weight trend sparkline + body composition gauge
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
    // Official Nord Palette
    private val NORD2 = Color.parseColor("#434C5E")
    private val NORD3 = Color.parseColor("#4C566A")
    private val NORD4 = Color.parseColor("#D8DEE9")
    private val NORD6 = Color.parseColor("#ECEFF4")
    private val NORD7 = Color.parseColor("#8FBCBB")
    private val NORD8 = Color.parseColor("#88C0D0")
    private val NORD9 = Color.parseColor("#81A1C1")
    private val NORD10 = Color.parseColor("#5E81AC")
    private val NORD11 = Color.parseColor("#BF616A")

    private const val CHART_W = 460
    private const val CHART_H = 190

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
    // 1. Steps & Heart 4x2 Widget (with 7-Day Steps Bar Chart + Heart Sparkline)
    // -----------------------------------------------------------------------
    fun updateActivityWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 101)

        val root = loadStore(context)
        val daysObj = root?.optJSONObject("days")
        if (daysObj == null) {
            views.setTextViewText(R.id.widget_left_main, "0 steps")
            views.setTextViewText(R.id.widget_left_sub, "0.0 km • 0 kcal")
            views.setTextViewText(R.id.widget_right_main, "RHR – bpm")
            views.setTextViewText(R.id.widget_right_sub, "HRV – • Ex 0m")
            views.setImageViewBitmap(R.id.widget_left_chart, drawStepsChart(emptyList()))
            views.setImageViewBitmap(R.id.widget_right_chart, drawSparklineChart(emptyList(), NORD8))
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val nf = NumberFormat.getIntegerInstance(Locale.US)
        val keys = daysObj.keys().asSequence().toList().sorted()
        val bestKey = keys.lastOrNull { k ->
            val d = daysObj.optJSONObject(k)
            d != null && (!d.isNull("steps") || !d.isNull("resting_hr") || !d.isNull("hrv_rmssd"))
        } ?: keys.lastOrNull()

        val stepHistory = keys.takeLast(7).map { k ->
            val item = daysObj.optJSONObject(k)
            if (item != null && !item.isNull("steps")) item.optLong("steps", 0L).toFloat() else 0f
        }

        val rhrHistory = mutableListOf<Float>()
        for (k in keys.takeLast(14)) {
            val item = daysObj.optJSONObject(k) ?: continue
            if (!item.isNull("resting_hr")) {
                val v = item.optDouble("resting_hr", 0.0).toFloat()
                if (v > 0f) rhrHistory.add(v)
            }
        }

        if (bestKey == null) {
            views.setTextViewText(R.id.widget_left_main, "0 steps")
            views.setTextViewText(R.id.widget_left_sub, "0.0 km • 0 kcal")
            views.setTextViewText(R.id.widget_right_main, "RHR – bpm")
            views.setTextViewText(R.id.widget_right_sub, "HRV – • Ex 0m")
        } else {
            val d = daysObj.optJSONObject(bestKey)
            val stepsVal = if (d != null && !d.isNull("steps")) d.optLong("steps", 0L) else 0L
            val steps = "${nf.format(stepsVal)} steps"

            val distKm = if (d != null && !d.isNull("distance_m")) {
                String.format(Locale.US, "%.1f km", d.optDouble("distance_m", 0.0) / 1000.0)
            } else "0.0 km"

            val kcal = if (d != null && !d.isNull("active_kcal")) {
                String.format(Locale.US, "%.0f kcal", d.optDouble("active_kcal", 0.0))
            } else "0 kcal"

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

            val rhrMain = rhrVal?.let { String.format(Locale.US, "RHR %.0f bpm", it) } ?: "RHR – bpm"
            val hrvStr = hrvVal?.let { String.format(Locale.US, "HRV %.0f ms", it) } ?: "HRV –"
            val exStr = if (d != null && !d.isNull("exercise_min")) {
                String.format(Locale.US, "Ex %.0fm", d.optDouble("exercise_min", 0.0))
            } else "Ex 0m"

            views.setTextViewText(R.id.widget_left_main, steps)
            views.setTextViewText(R.id.widget_left_sub, "$distKm • $kcal")
            views.setTextViewText(R.id.widget_right_main, rhrMain)
            views.setTextViewText(R.id.widget_right_sub, "$hrvStr • $exStr")
        }

        views.setImageViewBitmap(R.id.widget_left_chart, drawStepsChart(stepHistory))
        views.setImageViewBitmap(R.id.widget_right_chart, drawSparklineChart(rhrHistory, NORD8))
        mgr.updateAppWidget(widgetId, views)
    }

    // -----------------------------------------------------------------------
    // 2. Last Sleep 4x2 Widget (with 7-Night Sleep Bars + Stage Breakdown Bars)
    // -----------------------------------------------------------------------
    fun updateSleepWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 102)

        val root = loadStore(context)
        val sleepArr = root?.optJSONArray("sleep")
        if (sleepArr == null || sleepArr.length() == 0) {
            views.setTextViewText(R.id.widget_left_main, "No sleep")
            views.setTextViewText(R.id.widget_left_sub, "Bedtime –")
            views.setTextViewText(R.id.widget_right_main, "Deep –")
            views.setTextViewText(R.id.widget_right_sub, "REM – • Light –")
            views.setImageViewBitmap(R.id.widget_left_chart, drawSleepBarsChart(emptyList()))
            views.setImageViewBitmap(R.id.widget_right_chart, drawSleepStagesChart(0f, 0f, 0f, 0f))
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val recentHours = mutableListOf<Float>()
        val startIdx = max(0, sleepArr.length() - 7)
        var lastDeep = 0f
        var lastRem = 0f
        var lastLight = 0f
        var lastAwake = 0f
        var lastBedClock = "00:00"
        var lastInBedMin = 0.0
        var lastAsleepMin = 0.0

        for (idx in startIdx until sleepArr.length()) {
            val sObj = sleepArr.optJSONObject(idx) ?: continue
            val startMs = sObj.optLong("start_ms", 0L)
            val endMs = sObj.optLong("end_ms", 0L)
            val inBedMin = max(0.0, (endMs - startMs) / 60000.0)
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
            recentHours.add((asleepMin / 60.0).toFloat())

            if (idx == sleepArr.length() - 1) {
                lastDeep = deepMin.toFloat()
                lastRem = remMin.toFloat()
                lastLight = lightMin.toFloat()
                lastAwake = awakeMin.toFloat()
                lastInBedMin = inBedMin
                lastAsleepMin = asleepMin
                val bedMin = sObj.optInt("bedtime_min", 0).let { ((it % 1440) + 1440) % 1440 }
                lastBedClock = String.format(Locale.US, "%02d:%02d", bedMin / 60, bedMin % 60)
            }
        }

        val asleepStr = String.format(
            Locale.US,
            "%dh %02dm",
            lastAsleepMin.toInt() / 60,
            lastAsleepMin.toInt() % 60
        )
        val inBedStr = String.format(
            Locale.US,
            "%dh %02dm",
            lastInBedMin.toInt() / 60,
            lastInBedMin.toInt() % 60
        )
        val deepHero = if (lastDeep >= 60f) {
            String.format(Locale.US, "Deep %dh %02dm", lastDeep.toInt() / 60, lastDeep.toInt() % 60)
        } else {
            String.format(Locale.US, "Deep %.0fm", lastDeep)
        }

        views.setTextViewText(R.id.widget_left_main, asleepStr)
        views.setTextViewText(R.id.widget_left_sub, "Bed $lastBedClock • Tot $inBedStr")
        views.setTextViewText(R.id.widget_right_main, deepHero)
        views.setTextViewText(
            R.id.widget_right_sub,
            String.format(Locale.US, "REM %.0fm • Light %.0fm", lastRem, lastLight)
        )

        views.setImageViewBitmap(R.id.widget_left_chart, drawSleepBarsChart(recentHours))
        views.setImageViewBitmap(
            R.id.widget_right_chart,
            drawSleepStagesChart(lastDeep, lastRem, lastLight, lastAwake)
        )
        mgr.updateAppWidget(widgetId, views)
    }

    // -----------------------------------------------------------------------
    // 3. Last Weigh-in (Xiaomi Scale S400) 4x2 Widget (with Weight Trend + Body Comp Gauge)
    // -----------------------------------------------------------------------
    fun updateScaleWidget(context: Context, mgr: AppWidgetManager, widgetId: Int) {
        val views = RemoteViews(context.packageName, R.layout.widget_health_hub)
        attachLaunchIntent(context, views, 103)

        val root = loadStore(context)
        val scaleArr = root?.optJSONArray("scale")
        if (scaleArr == null || scaleArr.length() == 0) {
            views.setTextViewText(R.id.widget_left_main, "– kg")
            views.setTextViewText(R.id.widget_left_sub, "BMI – • Muscle –")
            views.setTextViewText(R.id.widget_right_main, "– % fat")
            views.setTextViewText(R.id.widget_right_sub, "Visceral – • Water –%")
            views.setImageViewBitmap(R.id.widget_left_chart, drawSparklineChart(emptyList(), NORD8))
            views.setImageViewBitmap(R.id.widget_right_chart, drawBodyCompGauge(0f, 0f, 0f, 0f))
            mgr.updateAppWidget(widgetId, views)
            return
        }

        val weightHistory = mutableListOf<Float>()
        val startIdx = max(0, scaleArr.length() - 12)
        for (i in startIdx until scaleArr.length()) {
            val item = scaleArr.optJSONObject(i) ?: continue
            val wVal = item.optDouble("weight_kg", 0.0).toFloat()
            if (wVal > 0f) weightHistory.add(wVal)
        }

        val mObj = scaleArr.optJSONObject(scaleArr.length() - 1)
        if (mObj != null) {
            val w = mObj.optDouble("weight_kg", 0.0).toFloat()
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

            views.setTextViewText(R.id.widget_left_main, String.format(Locale.US, "%.1f kg", w))
            views.setImageViewBitmap(R.id.widget_left_chart, drawSparklineChart(weightHistory, NORD8))

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
                    String.format(Locale.US, "%.1f%% fat", bf)
                )
                views.setTextViewText(
                    R.id.widget_right_sub,
                    String.format(Locale.US, "Visceral %.0f • Water %.1f%%", vf, water)
                )
                views.setImageViewBitmap(
                    R.id.widget_right_chart,
                    drawBodyCompGauge(bf, muscle, fatMass, bone)
                )
            } else {
                views.setTextViewText(
                    R.id.widget_left_sub,
                    String.format(Locale.US, "BMI %.1f", bmi)
                )
                views.setTextViewText(R.id.widget_right_main, "– % fat")
                views.setTextViewText(R.id.widget_right_sub, "Step barefoot for BIA")
                views.setImageViewBitmap(
                    R.id.widget_right_chart,
                    drawBodyCompGauge(0f, 0f, 0f, 0f)
                )
            }
        }

        mgr.updateAppWidget(widgetId, views)
    }

    // =======================================================================
    // Nord Graphic Renderers (High-DPI Anti-Aliased Canvas Bitmaps)
    // =======================================================================

    /**
     * 7-Day Steps Rounded Bar Chart + Today's Goal Progress Bar at bottom.
     */
    private fun drawStepsChart(values: List<Float>): Bitmap {
        val bmp = Bitmap.createBitmap(CHART_W, CHART_H, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        val padded = if (values.size >= 7) {
            values.takeLast(7)
        } else {
            List(7 - values.size) { 0f } + values
        }
        val maxVal = max(10000f, (padded.maxOrNull() ?: 10000f) * 1.08f)

        val chartTop = 10f
        val chartBottom = CHART_H - 34f
        val chartH = chartBottom - chartTop

        // 10k Goal dashed line
        val goalY = chartBottom - (10000f / maxVal).coerceIn(0f, 1f) * chartH
        paint.color = NORD3
        paint.strokeWidth = 2.5f
        var gx = 6f
        while (gx < CHART_W - 6f) {
            canvas.drawLine(gx, goalY, min(gx + 12f, CHART_W - 6f), goalY, paint)
            gx += 22f
        }

        // 7 Rounded Bars
        val count = 7
        val totalW = CHART_W - 12f
        val slotW = totalW / count
        val barW = slotW * 0.58f

        for (i in 0 until count) {
            val v = padded[i]
            val ratio = (v / maxVal).coerceIn(0.08f, 1f)
            val bh = ratio * chartH
            val cx = 6f + i * slotW + slotW * 0.5f
            val left = cx - barW * 0.5f
            val right = cx + barW * 0.5f
            val top = chartBottom - bh

            // Track background
            paint.style = Paint.Style.FILL
            paint.color = NORD2
            canvas.drawRoundRect(RectF(left, chartTop, right, chartBottom), 12f, 12f, paint)

            // Filled bar
            paint.color = when {
                i == count - 1 -> NORD8
                v >= 10000f -> NORD7
                v >= 7500f -> NORD9
                else -> NORD10
            }
            canvas.drawRoundRect(RectF(left, top, right, chartBottom), 12f, 12f, paint)

            // Snow Storm highlight cap on latest bar
            if (i == count - 1 && v > 0f) {
                paint.color = NORD6
                canvas.drawCircle(cx, min(top + 10f, chartBottom - 8f), 4.5f, paint)
            }
        }

        // Bottom 10k Goal Progress Bar
        val latestSteps = padded.lastOrNull() ?: 0f
        val progRatio = (latestSteps / 10000f).coerceIn(0.04f, 1f)
        val barTop = CHART_H - 16f
        val barBot = CHART_H - 4f
        paint.color = NORD2
        canvas.drawRoundRect(RectF(6f, barTop, CHART_W - 6f, barBot), 6f, 6f, paint)
        paint.color = if (latestSteps >= 10000f) NORD7 else NORD8
        canvas.drawRoundRect(
            RectF(6f, barTop, 6f + (CHART_W - 12f) * progRatio, barBot),
            6f,
            6f,
            paint
        )

        return bmp
    }

    /**
     * Smooth Nord Frost Sparkline Area Chart (used for Resting HR and Weight trend).
     */
    private fun drawSparklineChart(values: List<Float>, accentColor: Int): Bitmap {
        val bmp = Bitmap.createBitmap(CHART_W, CHART_H, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        val padX = 14f
        val topY = 16f
        val botY = CHART_H - 16f
        val plotW = CHART_W - padX * 2f
        val plotH = botY - topY

        // Subtle baseline grid lines
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = 2f
        paint.color = NORD2
        canvas.drawLine(padX, topY + plotH * 0.25f, CHART_W - padX, topY + plotH * 0.25f, paint)
        canvas.drawLine(padX, topY + plotH * 0.75f, CHART_W - padX, topY + plotH * 0.75f, paint)

        val pts = when {
            values.isEmpty() -> listOf(0.5f, 0.5f, 0.5f, 0.5f)
            values.size == 1 -> listOf(values[0], values[0], values[0])
            else -> values
        }

        val minV = pts.minOrNull() ?: 0f
        val maxV = pts.maxOrNull() ?: 1f
        val span = max(1.2f, maxV - minV)
        val low = minV - span * 0.2f
        val high = maxV + span * 0.2f

        val coords = pts.mapIndexed { idx, v ->
            val x = padX + (idx.toFloat() / max(1, pts.size - 1)) * plotW
            val norm = ((v - low) / (high - low)).coerceIn(0.1f, 0.9f)
            val y = botY - norm * plotH
            Pair(x, y)
        }

        val linePath = Path()
        val areaPath = Path()
        coords.forEachIndexed { idx, (x, y) ->
            if (idx == 0) {
                linePath.moveTo(x, y)
                areaPath.moveTo(x, botY)
                areaPath.lineTo(x, y)
            } else {
                val (px, py) = coords[idx - 1]
                val cx = (px + x) * 0.5f
                linePath.cubicTo(cx, py, cx, y, x, y)
                areaPath.cubicTo(cx, py, cx, y, x, y)
            }
        }
        areaPath.lineTo(coords.last().first, botY)
        areaPath.close()

        // Frost gradient fill under curve
        val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.FILL
            shader = LinearGradient(
                0f,
                topY,
                0f,
                botY,
                Color.argb(125, Color.red(accentColor), Color.green(accentColor), Color.blue(accentColor)),
                Color.argb(10, Color.red(accentColor), Color.green(accentColor), Color.blue(accentColor)),
                Shader.TileMode.CLAMP
            )
        }
        canvas.drawPath(areaPath, fillPaint)

        // Crisp Frost curve stroke
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = 6f
        paint.strokeCap = Paint.Cap.ROUND
        paint.strokeJoin = Paint.Join.ROUND
        paint.color = accentColor
        canvas.drawPath(linePath, paint)

        // Data points & Snow Storm bullseye on latest point
        if (values.isNotEmpty()) {
            paint.style = Paint.Style.FILL
            for (i in 0 until coords.size - 1) {
                paint.color = NORD9
                canvas.drawCircle(coords[i].first, coords[i].second, 5.5f, paint)
            }
            val (lx, ly) = coords.last()
            paint.color = NORD6
            canvas.drawCircle(lx, ly, 10.5f, paint)
            paint.color = accentColor
            canvas.drawCircle(lx, ly, 5.5f, paint)
        }

        return bmp
    }

    /**
     * 7-Night Sleep Duration Rounded Bar Chart with 7h–9h target band.
     */
    private fun drawSleepBarsChart(hours: List<Float>): Bitmap {
        val bmp = Bitmap.createBitmap(CHART_W, CHART_H, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        val padded = if (hours.size >= 7) {
            hours.takeLast(7)
        } else {
            List(7 - hours.size) { 0f } + hours
        }
        val maxH = max(9.5f, (padded.maxOrNull() ?: 9f) * 1.08f)

        val topY = 10f
        val botY = CHART_H - 10f
        val chartH = botY - topY

        // 8h optimal reference line
        val optY = botY - (8f / maxH) * chartH
        paint.color = NORD3
        paint.strokeWidth = 2.5f
        var gx = 6f
        while (gx < CHART_W - 6f) {
            canvas.drawLine(gx, optY, min(gx + 12f, CHART_W - 6f), optY, paint)
            gx += 22f
        }

        val count = 7
        val slotW = (CHART_W - 12f) / count
        val barW = slotW * 0.58f

        for (i in 0 until count) {
            val h = padded[i]
            val ratio = (h / maxH).coerceIn(0.08f, 1f)
            val bh = ratio * chartH
            val cx = 6f + i * slotW + slotW * 0.5f
            val left = cx - barW * 0.5f
            val right = cx + barW * 0.5f
            val top = botY - bh

            paint.style = Paint.Style.FILL
            paint.color = NORD2
            canvas.drawRoundRect(RectF(left, topY, right, botY), 12f, 12f, paint)

            paint.color = when {
                i == count - 1 -> NORD8
                h in 7f..9.2f -> NORD7
                h >= 6f -> NORD9
                else -> NORD10
            }
            canvas.drawRoundRect(RectF(left, top, right, botY), 12f, 12f, paint)

            if (i == count - 1 && h > 0f) {
                paint.color = NORD6
                canvas.drawCircle(cx, min(top + 10f, botY - 8f), 4.5f, paint)
            }
        }

        return bmp
    }

    /**
     * Sleep Stages Stacked Pill Bar + 4 Stage Proportion Mini-Bars (Deep, REM, Light, Awake).
     */
    private fun drawSleepStagesChart(deep: Float, rem: Float, light: Float, awake: Float): Bitmap {
        val bmp = Bitmap.createBitmap(CHART_W, CHART_H, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        val total = max(1f, deep + rem + light + awake)
        val padX = 6f
        val fullW = CHART_W - padX * 2f

        // 1. Top Stacked Sleep Stage Bar
        val barTop = 10f
        val barBot = 44f
        paint.style = Paint.Style.FILL
        paint.color = NORD2
        canvas.drawRoundRect(RectF(padX, barTop, CHART_W - padX, barBot), 16f, 16f, paint)

        val segments = listOf(
            Pair(deep, NORD10),  // Deep: Frost Deep Blue
            Pair(rem, NORD8),    // REM: Frost Ice Cyan
            Pair(light, NORD7),  // Light: Frost Teal
            Pair(awake, NORD4)   // Awake: Snow Storm
        )

        var curX = padX
        for ((mins, col) in segments) {
            val segW = (mins / total) * fullW
            if (segW > 2f) {
                paint.color = col
                canvas.drawRoundRect(
                    RectF(curX, barTop, min(curX + segW, CHART_W - padX), barBot),
                    10f,
                    10f,
                    paint
                )
                curX += segW
            }
        }

        // 2. Two-column 2x2 stage progress mini-bars below
        val labels = listOf(
            Triple("Deep", deep, NORD10),
            Triple("REM", rem, NORD8),
            Triple("Light", light, NORD7),
            Triple("Awake", awake, NORD4)
        )
        val colW = (fullW - 20f) * 0.5f
        val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = NORD4
            textSize = 24f
            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        }

        for (i in labels.indices) {
            val (name, mins, col) = labels[i]
            val colIdx = i % 2
            val rowIdx = i / 2
            val x0 = padX + colIdx * (colW + 20f)
            val y0 = 76f + rowIdx * 58f

            // Label + minutes
            canvas.drawText(
                String.format(Locale.US, "%s %.0fm", name, mins),
                x0,
                y0,
                textPaint
            )

            // Mini progress bar
            val pTop = y0 + 10f
            val pBot = pTop + 14f
            paint.color = NORD2
            canvas.drawRoundRect(RectF(x0, pTop, x0 + colW, pBot), 7f, 7f, paint)
            val fillW = (mins / total).coerceIn(0.06f, 1f) * colW
            paint.color = col
            canvas.drawRoundRect(RectF(x0, pTop, x0 + fillW, pBot), 7f, 7f, paint)
        }

        return bmp
    }

    /**
     * Body Fat Spectrum Gauge + Muscle/Fat/Bone Composition Bar for Xiaomi Scale S400.
     */
    private fun drawBodyCompGauge(
        bodyFatPct: Float,
        muscleKg: Float,
        fatKg: Float,
        boneKg: Float
    ): Bitmap {
        val bmp = Bitmap.createBitmap(CHART_W, CHART_H, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        val padX = 8f
        val fullW = CHART_W - padX * 2f

        // 1. Body Fat Spectrum Gauge (5% .. 40%)
        val gTop = 26f
        val gBot = 56f
        val zones = listOf(
            Pair(0.22f, NORD9),  // Low
            Pair(0.36f, NORD7),  // Healthy (Frost Teal)
            Pair(0.22f, NORD10), // High (Frost Deep Blue)
            Pair(0.20f, NORD11)  // Very High (Aurora Red)
        )
        var zx = padX
        for ((frac, col) in zones) {
            val zw = frac * fullW
            paint.color = col
            canvas.drawRoundRect(RectF(zx, gTop, zx + zw - 4f, gBot), 10f, 10f, paint)
            zx += zw
        }

        // Pointer pill on the Body Fat % spectrum
        if (bodyFatPct > 0f) {
            val norm = ((bodyFatPct - 6f) / 34f).coerceIn(0.03f, 0.97f)
            val px = padX + norm * fullW
            paint.color = NORD6
            canvas.drawRoundRect(RectF(px - 6f, gTop - 10f, px + 6f, gBot + 10f), 6f, 6f, paint)
        }

        // 2. Body Mass Composition Split Bar (Muscle vs Fat vs Bone)
        val totalMass = max(1f, muscleKg + fatKg + boneKg)
        val mTop = 104f
        val mBot = 136f
        paint.color = NORD2
        canvas.drawRoundRect(RectF(padX, mTop, CHART_W - padX, mBot), 14f, 14f, paint)

        if (muscleKg > 0f) {
            val mW = (muscleKg / totalMass) * fullW
            val fW = (fatKg / totalMass) * fullW
            val bW = (boneKg / totalMass) * fullW

            paint.color = NORD8 // Muscle: Bright Frost Cyan
            canvas.drawRoundRect(RectF(padX, mTop, padX + mW, mBot), 12f, 12f, paint)

            paint.color = NORD10 // Fat: Deep Frost Blue
            canvas.drawRoundRect(RectF(padX + mW, mTop, padX + mW + fW, mBot), 8f, 8f, paint)

            paint.color = NORD4 // Bone: Snow Storm
            canvas.drawRoundRect(
                RectF(padX + mW + fW, mTop, min(CHART_W - padX, padX + mW + fW + bW), mBot),
                8f,
                8f,
                paint
            )
        }

        // Legend row at bottom
        val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = NORD4
            textSize = 23f
            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        }
        val items = listOf(
            Pair("Muscle", NORD8),
            Pair("Fat", NORD10),
            Pair("Bone", NORD4)
        )
        val itemSlot = fullW / 3f
        for (i in items.indices) {
            val (lbl, col) = items[i]
            val lx = padX + i * itemSlot
            paint.color = col
            canvas.drawCircle(lx + 10f, 168f, 8f, paint)
            canvas.drawText(lbl, lx + 26f, 176f, textPaint)
        }

        return bmp
    }
}

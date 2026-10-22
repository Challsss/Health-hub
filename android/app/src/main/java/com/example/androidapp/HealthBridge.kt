package com.example.androidapp

import android.content.Context
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.*
import androidx.health.connect.client.request.AggregateRequest
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

object HealthBridge {
    val PERMISSIONS: Set<String> = setOf(
        HealthPermission.getReadPermission(StepsRecord::class),
        HealthPermission.getReadPermission(DistanceRecord::class),
        HealthPermission.getReadPermission(ActiveCaloriesBurnedRecord::class),
        HealthPermission.getReadPermission(TotalCaloriesBurnedRecord::class),
        HealthPermission.getReadPermission(ExerciseSessionRecord::class),
        HealthPermission.getReadPermission(HeartRateRecord::class),
        HealthPermission.getReadPermission(RestingHeartRateRecord::class),
        HealthPermission.getReadPermission(HeartRateVariabilityRmssdRecord::class),
        HealthPermission.getReadPermission(SleepSessionRecord::class),
        HealthPermission.getReadPermission(OxygenSaturationRecord::class),
        HealthPermission.getReadPermission(RespiratoryRateRecord::class),
        HealthPermission.getReadPermission(WeightRecord::class),
        HealthPermission.getReadPermission(BodyFatRecord::class),
    )

    fun isAvailable(context: Context): Boolean {
        return HealthConnectClient.getSdkStatus(context) == HealthConnectClient.SDK_AVAILABLE
    }

    fun hasPermissions(context: Context): Boolean = runBlocking {
        if (!isAvailable(context)) return@runBlocking false
        try {
            val client = HealthConnectClient.getOrCreate(context)
            val granted = client.permissionController.getGrantedPermissions()
            granted.isNotEmpty()
        } catch (_: Throwable) {
            false
        }
    }

    fun grantedCount(context: Context): Int = runBlocking {
        if (!isAvailable(context)) return@runBlocking 0
        try {
            val client = HealthConnectClient.getOrCreate(context)
            val granted = client.permissionController.getGrantedPermissions()
            granted.intersect(PERMISSIONS).size
        } catch (_: Throwable) {
            0
        }
    }

    fun readAllJson(context: Context, startMs: Long, endMs: Long): String = runBlocking {
        val root = JSONObject()
        val daysArr = JSONArray()
        val sleepArr = JSONArray()
        val hrvArr = JSONArray()
        val spo2Arr = JSONArray()
        val respArr = JSONArray()
        val weightArr = JSONArray()
        val bodyFatArr = JSONArray()
        val errorsArr = JSONArray()

        if (!isAvailable(context)) {
            errorsArr.put("Health Connect is not available on this device.")
            root.put("errors", errorsArr)
            return@runBlocking root.toString()
        }

        val client = HealthConnectClient.getOrCreate(context)
        val zone = ZoneId.systemDefault()
        val startDate: LocalDate = Instant.ofEpochMilli(startMs).atZone(zone).toLocalDate()
        val endDate: LocalDate = Instant.ofEpochMilli(endMs).atZone(zone).toLocalDate()

        var d = startDate
        while (!d.isAfter(endDate)) {
            val dayStart = d.atStartOfDay(zone).toInstant()
            val dayEnd = d.plusDays(1).atStartOfDay(zone).toInstant()
            try {
                val agg = client.aggregate(
                    AggregateRequest(
                        metrics = setOf(
                            StepsRecord.COUNT_TOTAL,
                            DistanceRecord.DISTANCE_TOTAL,
                            ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL,
                            TotalCaloriesBurnedRecord.ENERGY_TOTAL,
                            ExerciseSessionRecord.EXERCISE_DURATION_TOTAL,
                            HeartRateRecord.BPM_AVG,
                            HeartRateRecord.BPM_MIN,
                            HeartRateRecord.BPM_MAX,
                            RestingHeartRateRecord.BPM_AVG,
                        ),
                        timeRangeFilter = TimeRangeFilter.between(dayStart, dayEnd),
                    )
                )
                val dayObj = JSONObject().apply {
                    put("date", d.toString())
                    agg[StepsRecord.COUNT_TOTAL]?.let { put("steps", it) }
                    agg[DistanceRecord.DISTANCE_TOTAL]?.inMeters?.let { put("distanceM", it) }
                    agg[ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL]?.inKilocalories?.let {
                        put("activeKcal", it)
                    }
                    agg[TotalCaloriesBurnedRecord.ENERGY_TOTAL]?.inKilocalories?.let {
                        put("totalKcal", it)
                    }
                    agg[ExerciseSessionRecord.EXERCISE_DURATION_TOTAL]?.toMinutes()?.let {
                        put("exerciseMin", it.toDouble())
                    }
                    agg[HeartRateRecord.BPM_AVG]?.let { put("avgHr", it.toDouble()) }
                    agg[HeartRateRecord.BPM_MIN]?.let { put("minHr", it.toDouble()) }
                    agg[HeartRateRecord.BPM_MAX]?.let { put("maxHr", it.toDouble()) }
                    agg[RestingHeartRateRecord.BPM_AVG]?.let { put("restingHr", it.toDouble()) }
                }
                if (dayObj.length() > 1) {
                    daysArr.put(dayObj)
                }
            } catch (e: Throwable) {
                errorsArr.put("Aggregate ${d}: ${e.message}")
            }
            d = d.plusDays(1)
        }

        val range = TimeRangeFilter.between(
            Instant.ofEpochMilli(startMs),
            Instant.ofEpochMilli(endMs)
        )

        // Sleep sessions
        try {
            val sleepResp = client.readRecords(
                ReadRecordsRequest(SleepSessionRecord::class, timeRangeFilter = range)
            )
            for (s in sleepResp.records) {
                val endZoned = s.endTime.atZone(zone)
                val startZoned = s.startTime.atZone(zone)
                val bedtimeMin = startZoned.hour * 60 + startZoned.minute
                val stagesJson = JSONArray()
                for (st in s.stages) {
                    stagesJson.put(JSONObject().apply {
                        put("startMs", st.startTime.toEpochMilli())
                        put("endMs", st.endTime.toEpochMilli())
                        put("stage", st.stage)
                    })
                }
                sleepArr.put(JSONObject().apply {
                    put("startMs", s.startTime.toEpochMilli())
                    put("endMs", s.endTime.toEpochMilli())
                    put("date", endZoned.toLocalDate().toString())
                    put("bedtimeMin", bedtimeMin)
                    put("stages", stagesJson)
                    put("source", s.metadata.dataOrigin.packageName)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("Sleep: ${e.message}")
        }

        // HRV RMSSD
        try {
            val resp = client.readRecords(
                ReadRecordsRequest(HeartRateVariabilityRmssdRecord::class, timeRangeFilter = range)
            )
            for (r in resp.records) {
                val dt = r.time.atZone(zone).toLocalDate().toString()
                hrvArr.put(JSONObject().apply {
                    put("timeMs", r.time.toEpochMilli())
                    put("date", dt)
                    put("value", r.heartRateVariabilityMillis)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("HRV: ${e.message}")
        }

        // SpO2
        try {
            val resp = client.readRecords(
                ReadRecordsRequest(OxygenSaturationRecord::class, timeRangeFilter = range)
            )
            for (r in resp.records) {
                val dt = r.time.atZone(zone).toLocalDate().toString()
                spo2Arr.put(JSONObject().apply {
                    put("timeMs", r.time.toEpochMilli())
                    put("date", dt)
                    put("value", r.percentage.value)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("SpO2: ${e.message}")
        }

        // Respiratory rate
        try {
            val resp = client.readRecords(
                ReadRecordsRequest(RespiratoryRateRecord::class, timeRangeFilter = range)
            )
            for (r in resp.records) {
                val dt = r.time.atZone(zone).toLocalDate().toString()
                respArr.put(JSONObject().apply {
                    put("timeMs", r.time.toEpochMilli())
                    put("date", dt)
                    put("value", r.rate)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("RespRate: ${e.message}")
        }

        // Weight
        try {
            val resp = client.readRecords(
                ReadRecordsRequest(WeightRecord::class, timeRangeFilter = range)
            )
            for (r in resp.records) {
                weightArr.put(JSONObject().apply {
                    put("timeMs", r.time.toEpochMilli())
                    put("kg", r.weight.inKilograms)
                    put("source", r.metadata.dataOrigin.packageName)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("Weight: ${e.message}")
        }

        // Body fat
        try {
            val resp = client.readRecords(
                ReadRecordsRequest(BodyFatRecord::class, timeRangeFilter = range)
            )
            for (r in resp.records) {
                bodyFatArr.put(JSONObject().apply {
                    put("timeMs", r.time.toEpochMilli())
                    put("value", r.percentage.value)
                })
            }
        } catch (e: Throwable) {
            errorsArr.put("BodyFat: ${e.message}")
        }

        root.put("days", daysArr)
        root.put("sleep", sleepArr)
        root.put("hrv", hrvArr)
        root.put("spo2", spo2Arr)
        root.put("resp", respArr)
        root.put("weight", weightArr)
        root.put("bodyFat", bodyFatArr)
        root.put("errors", errorsArr)
        root.toString()
    }
}

package dev.iris.app.data

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

/**
 * Updating this app. The server tells the phone which release is newest (`GET /v1/update`,
 * which it got from GitHub); the phone downloads the APK itself, checks it, and hands it
 * to Android's installer, which asks for one tap.
 *
 * The APK is only used if all of these hold:
 *  - its address is exactly the project's GitHub release address for that version,
 *  - its SHA-256 equals the checksum GitHub lists for the file,
 *  - it is signed with the same key as the app already installed (Android would refuse
 *    otherwise; checking first gives a clear message instead of "App not installed").
 */
object AppUpdate {
    private const val REPO = "Joji228/IRIS-Intelligent-Reasoning-Initiative-System"
    private const val MAX_BYTES = 100L * 1024 * 1024

    private val http: OkHttpClient by lazy {
        OkHttpClient.Builder().connectTimeout(15, TimeUnit.SECONDS).readTimeout(60, TimeUnit.SECONDS).build()
    }

    /** "0.2.1" or "v0.2.1" as numbers; anything else is null. */
    fun parse(version: String): Triple<Int, Int, Int>? {
        val parts = version.trim().removePrefix("v").split('.')
        if (parts.size != 3) return null
        val n = parts.map { it.toIntOrNull() ?: return null }
        return Triple(n[0], n[1], n[2])
    }

    fun isNewer(latest: String, current: String): Boolean {
        val l = parse(latest) ?: return false
        val c = parse(current) ?: return false
        return compareValuesBy(l, c, { it.first }, { it.second }, { it.third }) > 0
    }

    /** The only address an APK may be fetched from for [version]. */
    fun expectedUrl(version: String) =
        "https://github.com/$REPO/releases/download/v$version/IRIS-Android-v$version.apk"

    /** The release and APK to offer, if a newer one than [current] is out and can be checked. */
    fun offer(status: UpdateStatus?, current: String): Pair<UpdateRelease, UpdateAsset>? {
        val release = status?.latest ?: return null
        val asset = release.android ?: return null
        if (!isNewer(release.version, current)) return null
        if (asset.sha256.isNullOrBlank() || asset.url != expectedUrl(release.version)) return null
        return release to asset
    }

    fun sha256Hex(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    /** Downloads the APK into the app's cache and checks its checksum; a file that fails is deleted. */
    suspend fun download(context: Context, release: UpdateRelease, asset: UpdateAsset, onProgress: (Int) -> Unit): File =
        withContext(Dispatchers.IO) {
            val expected = asset.sha256?.lowercase().orEmpty()
            check(asset.url == expectedUrl(release.version) && expected.length == 64) { "That download can't be checked, so it won't be installed." }
            val dir = File(context.cacheDir, "updates").apply { mkdirs() }
            dir.listFiles()?.forEach { it.delete() }
            val file = File(dir, asset.name)
            try {
                http.newCall(Request.Builder().url(asset.url).header("User-Agent", "iris-android").build()).execute().use { res ->
                    check(res.isSuccessful) { "GitHub answered ${res.code}" }
                    val body = checkNotNull(res.body) { "The download was empty" }
                    val total = body.contentLength()
                    check(total <= MAX_BYTES) { "The download is far bigger than an app should be" }
                    val digest = MessageDigest.getInstance("SHA-256")
                    var got = 0L
                    body.byteStream().use { input ->
                        file.outputStream().use { out ->
                            val buf = ByteArray(64 * 1024)
                            while (true) {
                                val n = input.read(buf)
                                if (n < 0) break
                                got += n
                                check(got <= MAX_BYTES) { "The download is far bigger than an app should be" }
                                digest.update(buf, 0, n)
                                out.write(buf, 0, n)
                                if (total > 0) onProgress((got * 100 / total).toInt())
                            }
                        }
                    }
                    val actual = digest.digest().joinToString("") { "%02x".format(it) }
                    check(actual == expected) { "The downloaded file doesn't match GitHub's checksum, so it was thrown away." }
                }
            } catch (e: Throwable) {
                file.delete()
                throw e
            }
            file
        }

    /** Whether the downloaded APK is signed by the same key as this app (so Android will update it in place). */
    fun signedLikeThisApp(context: Context, apk: File): Boolean {
        val pm = context.packageManager
        val flags = PackageManager.GET_SIGNING_CERTIFICATES
        val theirs = pm.getPackageArchiveInfo(apk.path, flags)?.signingInfo?.apkContentsSigners ?: return false
        val mine = pm.getPackageInfo(context.packageName, flags).signingInfo?.apkContentsSigners ?: return false
        return theirs.isNotEmpty() && theirs.all { t -> mine.any { it.toByteArray().contentEquals(t.toByteArray()) } }
    }

    /** Android must allow this app to install apps (a one-time switch in its settings). */
    fun canInstall(context: Context) = context.packageManager.canRequestPackageInstalls()

    fun allowInstallsIntent(context: Context): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    /** Opens Android's installer for the downloaded APK. */
    fun installIntent(context: Context, apk: File): Intent =
        Intent(Intent.ACTION_VIEW)
            .setDataAndType(FileProvider.getUriForFile(context, "${context.packageName}.files", apk), "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
}

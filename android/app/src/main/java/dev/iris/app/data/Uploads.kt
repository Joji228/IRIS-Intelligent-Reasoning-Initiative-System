package dev.iris.app.data

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import kotlin.math.max

/** A file picked on the phone, ready to upload with the next message. */
class Upload(val name: String, val bytes: ByteArray, val isImage: Boolean) {
    /** A small preview for the message box (images only). */
    val thumb: Bitmap? by lazy {
        if (!isImage) return@lazy null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        var sample = 1
        while (max(bounds.outWidth, bounds.outHeight) / (sample * 2) >= 200) sample *= 2
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
    }
}

object Uploads {
    /**
     * The most a file may be. A file travels base64-encoded inside a sealed, base64-encoded
     * link frame (about 1.8× its size), and OkHttp won't queue a frame over 16 MB.
     */
    const val MAX_BYTES = 8 * 1024 * 1024
    const val MAX_FILES = 10

    /** Photos are scaled to this on their long side (plenty for an agent to read them). */
    private const val PHOTO_EDGE = 2048

    class TooBig(val name: String) : Exception("$name is larger than ${MAX_BYTES / 1024 / 1024} MB")

    /** Reads a picked file; photos come back as a JPEG no larger than [PHOTO_EDGE]. */
    suspend fun read(context: Context, uri: Uri): Upload = withContext(Dispatchers.IO) {
        val resolver = context.contentResolver
        var name = "file"
        var size = -1L
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                c.getString(0)?.let { name = it }
                if (!c.isNull(1)) size = c.getLong(1)
            }
        }
        val mime = resolver.getType(uri).orEmpty()
        // A still photo is re-encoded smaller; GIFs keep their animation.
        if (mime.startsWith("image/") && mime != "image/gif") {
            runCatching { photo(context, uri, name) }.getOrNull()?.let { return@withContext it }
        }
        if (size > MAX_BYTES) throw TooBig(name)
        val bytes = resolver.openInputStream(uri)?.use { input ->
            val out = ByteArrayOutputStream()
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
                if (out.size() > MAX_BYTES) throw TooBig(name)
            }
            out.toByteArray()
        } ?: throw IllegalStateException("Couldn't open $name")
        Upload(name, bytes, mime.startsWith("image/"))
    }

    private fun photo(context: Context, uri: Uri, name: String): Upload {
        val source = ImageDecoder.createSource(context.contentResolver, uri)
        val bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            val edge = max(info.size.width, info.size.height)
            if (edge > PHOTO_EDGE) {
                val k = PHOTO_EDGE.toFloat() / edge
                decoder.setTargetSize((info.size.width * k).toInt(), (info.size.height * k).toInt())
            }
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
        val out = ByteArrayOutputStream()
        bitmap.compress(Bitmap.CompressFormat.JPEG, 86, out)
        bitmap.recycle()
        val base = name.substringBeforeLast('.', name).ifEmpty { "photo" }
        return Upload("$base.jpg", out.toByteArray(), true)
    }
}

package app.marrow.data

import android.content.Context
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import app.marrow.core.PlatformTokens
import com.google.android.gms.auth.GoogleAuthUtil
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import java.util.concurrent.TimeUnit

/** Addresses the browser comes back to after signing in (OneDrive, Dropbox). */
object CloudLinks {
    private val _incoming = MutableSharedFlow<String>(replay = 1, extraBufferCapacity = 4)
    val incoming: SharedFlow<String> = _incoming.asSharedFlow()

    fun arrived(uri: String) {
        _incoming.tryEmit(uri)
    }

    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    fun clear() = _incoming.resetReplayCache()

    /** Opens a sign-in page in the browser, inside the app (Custom Tabs). */
    fun open(context: Context, url: String) {
        CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, Uri.parse(url))
    }
}

/**
 * Google Drive on Android signs in through Google Play services: once the
 * person has agreed (on the sync screen), tokens come without asking again.
 */
class GoogleTokens(private val context: Context) : PlatformTokens {
    @Volatile private var last: String? = null

    override fun googleToken(renew: Boolean): String? = try {
        if (renew) last?.let { runCatching { GoogleAuthUtil.clearToken(context, it) } }
        val r = Tasks.await(Identity.getAuthorizationClient(context).authorize(request()), 30, TimeUnit.SECONDS)
        if (r.hasResolution()) null else r.accessToken?.also { last = it }
    } catch (e: Exception) {
        null
    }

    companion object {
        const val DRIVE_FILE = "https://www.googleapis.com/auth/drive.file"
        fun request(): AuthorizationRequest = AuthorizationRequest.builder().setRequestedScopes(listOf(Scope(DRIVE_FILE))).build()
    }
}

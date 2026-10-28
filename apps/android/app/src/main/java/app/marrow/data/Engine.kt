package app.marrow.data

import android.content.Context
import app.marrow.core.Marrow
import app.marrow.core.MarrowException
import app.marrow.core.MarrowListener
import app.marrow.core.SecretStore
import app.marrow.core.SyncStatus
import app.marrow.core.VaultInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The app's one link to Marrow's engine (the Rust core, through UniFFI).
 * Calls go through [run], off the main thread. Changes the engine reports
 * (saves, sync, other apps) come out of [changes], so screens reload.
 */
class Engine(
    context: Context,
    dataDir: File = File(context.filesDir, "marrow"),
    secrets: SecretStore = KeystoreSecrets(context),
) {
    val marrow: Marrow = Marrow(dataDir.absolutePath, secrets)
    val prefs = Prefs(context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val _vault = MutableStateFlow<VaultInfo?>(null)
    /** The open vault, or null before one is chosen. */
    val vault: StateFlow<VaultInfo?> = _vault.asStateFlow()

    private val _changes = MutableSharedFlow<List<String>>(extraBufferCapacity = 64)
    /** Paths that changed. Screens showing them reload. */
    val changes: SharedFlow<List<String>> = _changes.asSharedFlow()

    private val _sync = MutableStateFlow<SyncStatus?>(null)
    val sync: StateFlow<SyncStatus?> = _sync.asStateFlow()

    private val _locks = MutableStateFlow(0)
    /** Goes up each time a password is entered or forgotten. */
    val locks: StateFlow<Int> = _locks.asStateFlow()

    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 8)
    /** Short messages for a snackbar ("Saved to today's note"). */
    val messages: SharedFlow<String> = _messages.asSharedFlow()

    init {
        marrow.setPlatformTokens(GoogleTokens(context.applicationContext))
        marrow.setListener(object : MarrowListener {
            override fun vaultChanged(paths: List<String>) {
                _changes.tryEmit(paths)
            }

            override fun syncChanged(status: SyncStatus) {
                _sync.value = status
            }

            override fun locksChanged() {
                _locks.value += 1
            }
        })
    }

    /** Runs an engine call off the main thread. */
    suspend fun <T> run(block: Marrow.() -> T): T = withContext(Dispatchers.IO) { marrow.block() }

    fun say(message: String) {
        _messages.tryEmit(message)
    }

    /** Opens the vault last used, if it's still there. True if one is open. */
    suspend fun openLast(): Boolean = run {
        val last = lastVault() ?: return@run false
        try {
            _vault.value = openVault(last)
            _sync.value = syncStatus()
            true
        } catch (e: MarrowException) {
            false
        }
    }

    suspend fun open(path: String): VaultInfo = run {
        val info = openVault(path)
        _vault.value = info
        _sync.value = syncStatus()
        info
    }

    fun refreshSync() {
        _sync.value = marrow.syncStatus()
    }

    fun close() {
        marrow.closeVault()
        _vault.value = null
        _sync.value = null
    }

    fun background(block: suspend () -> Unit) {
        scope.launch { block() }
    }
}

/** Words for an error, to show the person. */
fun Throwable.words(): String = when (this) {
    is MarrowException.Failed -> reason
    is MarrowException.Locked -> "This note is locked."
    is MarrowException.LockedFolder -> "The folder “$folder” is locked."
    is MarrowException.ChangedElsewhere -> "This note was changed somewhere else."
    is MarrowException.NoVault -> "No vault is open."
    else -> message ?: toString()
}

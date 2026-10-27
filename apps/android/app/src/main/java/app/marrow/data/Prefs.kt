package app.marrow.data

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** How the app looks and behaves on this phone (not part of the vault). */
data class Look(
    /** "system", "light" or "dark". */
    val theme: String = "system",
    /** The font notes are read in: "book" (Literata), "sans" or "classic" (EB Garamond). */
    val readFont: String = "book",
    /** 0 small, 1 medium, 2 large, 3 larger. */
    val textSize: Int = 1,
    /** Notes open for reading ("read") or straight into editing ("edit"). */
    val openIn: String = "read",
    /** Forget passwords after this many minutes away (0 = when the app closes). */
    val lockAfter: Int = 5,
    /** "name", "modified" or "created". */
    val sort: String = "name",
)

class Prefs(context: Context) {
    private val p = context.getSharedPreferences("prefs", Context.MODE_PRIVATE)
    private val _look = MutableStateFlow(load())
    val look: StateFlow<Look> = _look.asStateFlow()

    private fun load() = Look(
        theme = p.getString("theme", "system")!!,
        readFont = p.getString("readFont", "book")!!,
        textSize = p.getInt("textSize", 1),
        openIn = p.getString("openIn", "read")!!,
        lockAfter = p.getInt("lockAfter", 5),
        sort = p.getString("sort", "name")!!,
    )

    fun update(change: (Look) -> Look) {
        val l = change(_look.value)
        p.edit()
            .putString("theme", l.theme)
            .putString("readFont", l.readFont)
            .putInt("textSize", l.textSize)
            .putString("openIn", l.openIn)
            .putInt("lockAfter", l.lockAfter)
            .putString("sort", l.sort)
            .apply()
        _look.value = l
    }

    /** Recent searches, newest first. */
    var recentSearches: List<String>
        get() = p.getString("recentSearches", "")!!.split('\n').filter { it.isNotBlank() }
        set(v) = p.edit().putString("recentSearches", v.take(8).joinToString("\n")).apply()

    /** Recent questions asked of the notes. */
    var recentQuestions: List<String>
        get() = p.getString("recentQuestions", "")!!.split('\n').filter { it.isNotBlank() }
        set(v) = p.edit().putString("recentQuestions", v.take(6).joinToString("\n")).apply()

    /** What to show once the vault opens ("sync" after "Sync from a server"). */
    var afterOpen: String?
        get() = p.getString("afterOpen", null)
        set(v) = p.edit().putString("afterOpen", v).apply()
}

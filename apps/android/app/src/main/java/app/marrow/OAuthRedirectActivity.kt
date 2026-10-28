package app.marrow

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import app.marrow.data.CloudLinks

/** Where the browser comes back after signing in: hands the address to the sync screen. */
class OAuthRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent?.data?.toString()?.let(CloudLinks::arrived)
        // Back to Marrow, closing the sign-in page.
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        finish()
    }
}

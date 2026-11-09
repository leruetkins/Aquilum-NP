package md.vibecodium.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import md.vibecodium.mobile.ui.VibecodiumApp

class MainActivity : ComponentActivity() {
    private lateinit var controller: VibecodiumController

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        controller = VibecodiumController(applicationContext)
        setContent {
            VibecodiumApp(controller)
        }
    }

    override fun onStop() {
        controller.flushBlocking()
        super.onStop()
    }

    override fun onDestroy() {
        controller.dispose()
        super.onDestroy()
    }
}

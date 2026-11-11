package md.vibecodium.mobile

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import md.vibecodium.mobile.ui.VibecodiumApp

class MainActivity : ComponentActivity() {
    private lateinit var controller: VibecodiumController

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSoftInputMode(
            WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
            WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE
        )
        enableEdgeToEdge()
        controller = VibecodiumController(applicationContext)
        setContent {
            VibecodiumApp(controller)
        }
    }

    override fun onResume() {
        super.onResume()
        window.setSoftInputMode(
            WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
            WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE
        )
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

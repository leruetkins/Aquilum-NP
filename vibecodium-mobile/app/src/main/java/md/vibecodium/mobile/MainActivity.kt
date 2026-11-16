package md.vibecodium.mobile

import android.graphics.Color
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
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
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
            // The system navigation bar follows the OS light/dark setting, not
            // the app's Obsidian theme, so switching themes never recolors it.
            navigationBarStyle = SystemBarStyle.dark(Color.BLACK),
        )
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

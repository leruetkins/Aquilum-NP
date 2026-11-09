package md.vibecodium.mobile

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.Socket

class DropboxAuthTest {
    @Test
    fun authUrlContainsPkceAndLoopbackRedirect() {
        val pkce = DropboxAuth.Pkce("verifier", "challenge", "state-1")
        val url = DropboxAuth.authUrl("client-1", pkce)

        assertTrue(url.startsWith("https://www.dropbox.com/oauth2/authorize?"))
        assertTrue(url.contains("client_id=client-1"))
        assertTrue(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A47219%2F"))
        assertTrue(url.contains("code_challenge=challenge"))
        assertTrue(url.contains("code_challenge_method=S256"))
        assertTrue(url.contains("state=state-1"))
        assertTrue(url.contains("token_access_type=offline"))
    }

    @Test
    fun loopbackReceivesCodeAndState() = runBlocking {
        val server = LoopbackServer()
        server.start()
        try {
            val code = async(Dispatchers.IO) { server.awaitCode("state-1", 5_000) }
            withContext(Dispatchers.IO) {
                Socket("127.0.0.1", 47219).use { socket ->
                    val request = "GET /?code=abc&state=state-1 HTTP/1.1\r\n" +
                        "Host: 127.0.0.1:47219\r\nConnection: close\r\n\r\n"
                    socket.getOutputStream().write(request.toByteArray(Charsets.US_ASCII))
                    socket.getOutputStream().flush()
                    socket.getInputStream().bufferedReader().readText()
                }
            }
            assertEquals("abc", code.await())
        } finally {
            server.close()
        }
    }

    @Test(expected = IllegalStateException::class)
    fun loopbackRejectsWrongState(): Unit = runBlocking {
        val server = LoopbackServer()
        server.start()
        try {
            val code = async(Dispatchers.IO) { server.awaitCode("expected", 5_000) }
            withContext(Dispatchers.IO) {
                Socket("127.0.0.1", 47219).use { socket ->
                    val request = "GET /?code=abc&state=wrong HTTP/1.1\r\n" +
                        "Host: 127.0.0.1:47219\r\nConnection: close\r\n\r\n"
                    socket.getOutputStream().write(request.toByteArray(Charsets.US_ASCII))
                    socket.getOutputStream().flush()
                    runCatching { socket.getInputStream().bufferedReader().readText() }
                }
            }
            code.await()
            Unit
        } finally {
            server.close()
        }
    }
}

package com.frostsnap

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.UserNotAuthenticatedException
import android.util.Log
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.MethodChannel.MethodCallHandler
import io.flutter.plugin.common.MethodChannel.Result
import io.flutter.plugin.common.PluginRegistry
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.Mac

class SecureKeyManager : FlutterPlugin, MethodCallHandler, ActivityAware, PluginRegistry.ActivityResultListener {
    private lateinit var channel: MethodChannel
    private var activity: Activity? = null
    private var pendingResult: Result? = null
    private val TAG = "SecureKeyManager"

    companion object {
        private const val CHANNEL = "com.frostsnap/secure_key"
        private const val KEY_ALIAS = "frostsnap-app-encryption"
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val REQUEST_CODE_CONFIRM_DEVICE_CREDENTIALS = 1
    }

    override fun onAttachedToEngine(flutterPluginBinding: FlutterPlugin.FlutterPluginBinding) {
        channel = MethodChannel(flutterPluginBinding.binaryMessenger, CHANNEL)
        channel.setMethodCallHandler(this)
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel.setMethodCallHandler(null)
    }

    override fun onMethodCall(call: MethodCall, result: Result) {
        when (call.method) {
            "getOrCreateKey" -> getOrCreateKey(result)
            "requiresAuthentication" -> requiresAuthentication(result)
            "clearKey" -> clearKey(result)
            "deleteKey" -> deleteKey(result)
            else -> result.notImplemented()
        }
    }

    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        activity = binding.activity
        binding.addActivityResultListener(this)
    }

    override fun onDetachedFromActivityForConfigChanges() {
        activity = null
    }

    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) {
        activity = binding.activity
        binding.addActivityResultListener(this)
    }

    override fun onDetachedFromActivity() {
        activity = null
    }

    private fun getOrCreateKey(result: Result) {
        val aliasExists =
            try {
                val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
                keyStore.load(null)
                keyStore.containsAlias(KEY_ALIAS)
            } catch (e: Exception) {
                reportKeyUseFailure(result, e)
                return
            }

        if (!aliasExists) {
            try {
                createKey()
            } catch (e: Exception) {
                Log.e(TAG, "Key generation failed", e)
                result.error(
                    "KEY_CREATION_FAILED",
                    "Couldn't create a secure key on this phone (${e.javaClass.simpleName}): ${e.message}",
                    null,
                )
                return
            }
        }

        deliverKeyBytes(result, allowAuthPrompt = true)
    }

    // The initial fetch and the post-authentication retry both come through here so
    // both entry points classify failures the same way.
    private fun deliverKeyBytes(result: Result?, allowAuthPrompt: Boolean) {
        val keyBytes =
            try {
                getKeyBytes()
            } catch (e: UserNotAuthenticatedException) {
                // Almost certainly unreachable; kept defensively rather than because it is
                // expected. Only keys minted under the old strict spec are auth-bound at all,
                // and that spec set setUserAuthenticationValidityDurationSeconds(
                // Integer.MAX_VALUE) — ~68 years — so a single unlock covers such a key's whole
                // practical life and the window never lapses. Keys minted since the relaxation
                // are not auth-bound and cannot raise this at all.
                if (allowAuthPrompt && result != null) {
                    Log.d(TAG, "Key requires authentication")
                    launchLockScreen(result)
                } else {
                    reportKeyUseFailure(result, e)
                }
                return
            } catch (e: KeyPermanentlyInvalidatedException) {
                // The old key was auth-bound and its credential is gone, so the alias is
                // a dead tombstone that blocks its own replacement. Nothing it encrypted
                // is recoverable from it, so delete and recreate it; the wallets it held
                // then read as wrong-key and drive per-wallet recovery.
                Log.e(TAG, "Key permanently invalidated; healing", e)
                healKey(result)
                return
            } catch (e: Exception) {
                reportKeyUseFailure(result, e)
                return
            }

        if (keyBytes != null) {
            result?.success(keyBytes)
        } else {
            reportKeyUseFailure(result, IllegalStateException("key access produced no bytes"))
        }
    }

    private fun reportKeyUseFailure(result: Result?, cause: Throwable) {
        Log.e(TAG, "Key retrieval failed", cause)
        result?.error(
            "KEY_USE_FAILED",
            "Couldn't unlock your wallet's encryption key on this phone (${cause.javaClass.simpleName}): ${cause.message}",
            null,
        )
    }

    private fun createKey() {
        val keyGenerator =
            KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_HMAC_SHA256, ANDROID_KEYSTORE)
        keyGenerator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_SIGN)
                .setKeySize(256)
                .setDigests(KeyProperties.DIGEST_SHA256)
                .build()
        )
        keyGenerator.generateKey()
    }

    private fun healKey(result: Result?) {
        val keyBytes =
            try {
                val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
                keyStore.load(null)
                if (keyStore.containsAlias(KEY_ALIAS)) {
                    keyStore.deleteEntry(KEY_ALIAS)
                }
                createKey()
                getKeyBytes()
            } catch (e: Exception) {
                Log.e(TAG, "Key heal failed", e)
                result?.error(
                    "KEY_CREATION_FAILED",
                    "Couldn't recreate a secure key on this phone (${e.javaClass.simpleName}): ${e.message}",
                    null,
                )
                return
            }

        if (keyBytes != null) {
            result?.success(keyBytes)
        } else {
            reportKeyUseFailure(result, IllegalStateException("key access produced no bytes"))
        }
    }

    private fun launchLockScreen(result: Result) {
        val activity = activity ?: run {
            result.error("NO_ACTIVITY", "Activity not available for authentication", null)
            return
        }

        val keyguardManager = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager

        if (!keyguardManager.isDeviceSecure) {
            // An auth-bound key whose lock screen was removed can never authenticate
            // again — a new lock screen is a new credential. Treat it as dead and heal.
            healKey(result)
            return
        }

        val intent = keyguardManager.createConfirmDeviceCredentialIntent(
            "Authenticate to Access Secure Key",
            "Your device credential is required"
        )

        if (intent != null) {
            pendingResult = result
            activity.startActivityForResult(intent, REQUEST_CODE_CONFIRM_DEVICE_CREDENTIALS)
        } else {
            result.error("LOCK_SCREEN_ERROR", "Could not create lock screen intent", null)
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        if (requestCode == REQUEST_CODE_CONFIRM_DEVICE_CREDENTIALS) {
            val result = pendingResult
            pendingResult = null

            if (resultCode == Activity.RESULT_OK) {
                Log.i(TAG, "Lock screen authentication succeeded")
                deliverKeyBytes(result, allowAuthPrompt = false)
            } else {
                Log.w(TAG, "Lock screen authentication cancelled")
                result?.error("AUTH_CANCELLED", "Authentication cancelled by user", null)
            }
            return true
        }
        return false
    }

    private fun getKeyBytes(): ByteArray? {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
        keyStore.load(null)

        val secretKey = keyStore.getKey(KEY_ALIAS, null) as SecretKey

        // Use HMAC to derive a consistent 32-byte key from the hardware-backed key
        // This is the proper way to do key derivation
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(secretKey)

        // Use a fixed input to ensure deterministic output
        return mac.doFinal("frostsnap-hmac-v0".toByteArray())
    }

    private fun requiresAuthentication(result: Result) {
        // With unlimited validity, authentication is only required once
        // We could check if the key can be accessed without throwing UserNotAuthenticatedException
        try {
            val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
            keyStore.load(null)

            if (!keyStore.containsAlias(KEY_ALIAS)) {
                // Key doesn't exist yet
                result.success(true)
                return
            }

            // Try to use the key
            getKeyBytes()
            result.success(false) // Key can be accessed without authentication
        } catch (e: UserNotAuthenticatedException) {
            result.success(true) // Authentication required
        } catch (e: Exception) {
            Log.e(TAG, "Error checking authentication requirement", e)
            result.success(true) // Assume authentication required on error
        }
    }

    private fun clearKey(result: Result) {
        // No-op since we don't cache authentication
        result.success(null)
    }

    private fun deleteKey(result: Result) {
        try {
            val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
            keyStore.load(null)

            if (keyStore.containsAlias(KEY_ALIAS)) {
                keyStore.deleteEntry(KEY_ALIAS)
                Log.i(TAG, "Key deleted successfully")
            }

            result.success(null)
        } catch (e: Exception) {
            Log.e(TAG, "Error deleting key", e)
            result.error("DELETE_ERROR", "Failed to delete key: ${e.message}", null)
        }
    }
}

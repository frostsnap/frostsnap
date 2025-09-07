package com.frostsnap

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
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
        try {
            val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE)
            keyStore.load(null)
            
            // Create key if it doesn't exist
            if (!keyStore.containsAlias(KEY_ALIAS)) {
                createKey()
            }
            
            // Try to access the key
            val keyBytes = getKeyBytes()
            if (keyBytes != null) {
                result.success(keyBytes)
            } else {
                // If we get here and keyBytes is null, it's likely due to authentication needed
                launchLockScreen(result)
            }
        } catch (e: UserNotAuthenticatedException) {
            // Key requires authentication
            Log.d(TAG, "Key requires authentication")
            launchLockScreen(result)
        } catch (e: Exception) {
            Log.e(TAG, "Error in getOrCreateKey", e)
            result.error("KEY_ERROR", "Failed to get or create key: ${e.message}", null)
        }
    }
    
    private fun createKey() {
        val keyGenerator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_HMAC_SHA256, ANDROID_KEYSTORE)
        
        fun buildKeySpec(useStrongBox: Boolean): KeyGenParameterSpec {
            val builder = KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_SIGN
            )
                .setKeySize(256)
                .setUserAuthenticationRequired(true)
            
            // Use the new API for Android 11+ (API 30+), fall back to deprecated method for older versions
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                builder.setUserAuthenticationParameters(
                    Integer.MAX_VALUE,
                    KeyProperties.AUTH_DEVICE_CREDENTIAL or KeyProperties.AUTH_BIOMETRIC_STRONG
                )
            } else {
                builder.setUserAuthenticationValidityDurationSeconds(Integer.MAX_VALUE)
            }
            
            // Set StrongBox if requested and available (API 28+)
            if (useStrongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                builder.setIsStrongBoxBacked(true)
            }
            
            return builder.build()
        }
        
        // Try to use StrongBox if available (API 28+)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                keyGenerator.init(buildKeySpec(useStrongBox = true))
                keyGenerator.generateKey()
                Log.i(TAG, "Key created successfully with StrongBox backing")
                return
            } catch (e: Exception) {
                // StrongBox failed, try without it
                Log.w(TAG, "StrongBox not available: ${e.message}, falling back to TEE")
            }
        }
        
        // Create key without StrongBox (for older devices or when StrongBox fails)
        keyGenerator.init(buildKeySpec(useStrongBox = false))
        keyGenerator.generateKey()
        Log.i(TAG, "Key created successfully with TEE backing")
    }
    
    private fun launchLockScreen(result: Result) {
        val activity = activity ?: run {
            result.error("NO_ACTIVITY", "Activity not available for authentication", null)
            return
        }
        
        val keyguardManager = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        
        if (!keyguardManager.isDeviceSecure) {
            result.error("NO_LOCK_SCREEN", "Device does not have a secure lock screen", null)
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
                // Authentication succeeded, try to get the key again
                Log.i(TAG, "Lock screen authentication succeeded")
                try {
                    val keyBytes = getKeyBytes()
                    if (keyBytes != null) {
                        result?.success(keyBytes)
                    } else {
                        result?.error("KEY_ERROR", "Failed to retrieve key after authentication", null)
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Error getting key after authentication", e)
                    result?.error("KEY_ERROR", "Failed to retrieve key: ${e.message}", null)
                }
            } else {
                // Authentication cancelled
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

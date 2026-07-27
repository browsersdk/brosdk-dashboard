#if !defined(__BROSDK_H__)
#define __BROSDK_H__
/* Minimal public C API header - keep footprint small but include C types
  needed by language bindings (cgo, ctypes, etc.). */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>

#if defined(SDK_API)
#undef SDK_API
#endif
#if defined(SDK_CALL)
#undef SDK_CALL
#endif

#if defined(_WIN32) || defined(_WIN64)
#define SDK_API __declspec(dllexport)
#define SDK_CALL __cdecl
#elif defined(__clang__) || defined(__GNUC__)
#define SDK_API __attribute__((visibility("default")))
#define SDK_CALL
#else
#define SDK_API
#define SDK_CALL
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque SDK instance handle.
 * - For pure C callers this value is usually only passed through.
 * - For C++ callers it can be cast to ISDK* after sdk_init / sdk_init_cpp. */
typedef void *sdk_handle_t;

typedef enum {
  SDK_LOG_TYPE_UNKNOWN = 0,
  /* A local SDK log line after the unified logger formats timestamp/prefix. */
  SDK_LOG_TYPE_LOCAL = 1,
  /* A structured SDK log item that is queued for upload to sdk-server. */
  SDK_LOG_TYPE_SERVER = 2,
} sdk_log_type_t;
typedef void(SDK_CALL *sdk_log_cb_t)(sdk_log_type_t type, const char *data,
                                     size_t len);
/* Global async result callback.
 * code      : coarse status code for this notification (OK / WARN / ERROR).
 * user_data : caller-provided pointer passed back unchanged.
 * data/len  : UTF-8 JSON payload. Parse this body for reqId / type / data.
 *
 * Important: the first parameter is NOT a stable reqId/eventId carrier.
 * Always treat the JSON body as the source of truth. */
typedef void(SDK_CALL *sdk_result_cb_t)(int32_t code, void *user_data,
                                        const char *data, size_t len);

/* Cookie persistence interception callback.
 * data/len         : UTF-8 JSON event object. The complete Cookie array is in
 *                    the data.cookies member.
 * new_data/new_len : optional replacement containing a complete JSON Cookie
 *                    array with at least one injectable Cookie, not an event
 *                    object or an incremental patch. Leave NULL/0 to keep the
 *                    original array. An empty, invalid, or fully rejected
 *                    replacement also keeps the original array; this callback
 *                    is not a clear-Cookie operation.
 * user_data        : caller-provided pointer passed back unchanged.
 *
 * If you replace the payload, allocate *new_data with sdk_malloc().
 * The SDK will release it with sdk_free(). */
typedef void(SDK_CALL *sdk_cookies_storage_cb_t)(const char *data, size_t len,
                                                 char **new_data,
                                                 size_t *new_len,
                                                 void *user_data);

/* Security strategy interception callback.
 * data/len           : JSON payload describing the blocked request.
 * redirect/new_len   : optional redirect URL. Leave NULL/0 to use the SDK's
 *                      default block behavior.
 * user_data          : caller-provided pointer passed back unchanged.
 *
 * If you return a redirect URL, allocate *redirect with sdk_malloc().
 * The SDK will release it with sdk_free(). */
typedef void(SDK_CALL *sdk_security_decision_cb_t)(const char *data, size_t len,
                                                   char **redirect,
                                                   size_t *redirect_len,
                                                   void *user_data);

/* Register the global async result callback.
 * Call this before sdk_init if you need to receive init/open/close events.
 * Pass NULL to disable result callback delivery. */
SDK_API int32_t SDK_CALL sdk_register_result_cb(sdk_result_cb_t cb,
                                                void *user_data);

/* Register the SDK log callback.
 * type     : distinguishes local SDK log lines from sdk-server upload logs.
 * data/len : UTF-8 payload; copy it before returning.
 *
 * Pass NULL to disable log callback delivery. */
SDK_API int32_t SDK_CALL sdk_register_log_cb(sdk_log_cb_t cb);

/* Register the cookie/storage interception callback.
 * Optional; only needed when the host wants to inspect or replace cookies
 * before the SDK persists them.
 * Pass NULL to disable cookie/storage interception. */
SDK_API int32_t SDK_CALL
sdk_register_cookies_storage_cb(sdk_cookies_storage_cb_t cb, void *user_data);

/* Register the security strategy decision callback.
 * Optional. When a bridge security rule blocks a request, this callback can
 * return a redirect URL. Without a redirect URL, the bridge returns its default
 * block response.
 * Pass NULL to disable security strategy interception. */
SDK_API int32_t SDK_CALL sdk_register_security_decision_cb(
    sdk_security_decision_cb_t cb, void *user_data);

/* Return the current SDK instance handle without performing init.
 * Mainly useful for C++ callers that need an ISDK* for an already-created
 * singleton. */
SDK_API int32_t SDK_CALL sdk_init_cpp(sdk_handle_t *);

/* Synchronous SDK initialization.
 * data/len         : UTF-8 JSON request body. Common fields:
 *                    userSig(required), workDir(optional), port(optional),
 *                    sdkApiUrl(optional), debug(optional).
 * cpp_handle       : optional output handle for C++ callers.
 * out_data/out_len : malloc'd UTF-8 JSON response; caller must sdk_free().
 *
 * Returns CL_OK on success or a negative error code on failure. */
SDK_API int32_t SDK_CALL sdk_init(sdk_handle_t *cpp_handle, const char *data,
                                  size_t len, char **out_data, size_t *out_len);

/* Asynchronous SDK initialization.
 * Returns a reqId when the request is accepted; final result is delivered via
 * sdk_result_cb or WebSocket push if the embedded server is enabled. */
SDK_API int32_t SDK_CALL sdk_init_async(sdk_handle_t *cpp_handle,
                                        const char *data, size_t len);

/* Compatibility helper that only enables the embedded Web API port.
 * New integrations should prefer sdk_init / sdk_init_async and include "port"
 * in the JSON request body so userSig and workDir can be supplied together. */
SDK_API int32_t SDK_CALL sdk_init_webapi(uint16_t port);

/* Read SDK runtime information synchronously.
 * Returns raw info JSON in *out_data (not the callback-style async envelope).
 */
SDK_API int32_t SDK_CALL sdk_info(char **out_data, size_t *out_len);
/*
 * Get a userSig for the current user. This is a synchronous call because the
 * caller typically needs the userSig before it can meaningfully proceed with
 * async initialization.
 *
 * data/len         : UTF-8 JSON body. Common fields:
 *                    apiKey(required), customerId(optional),
 *                    duration(optional).
 * out_data/out_len : raw backend getUserSig JSON; caller must
 * sdk_free(*out_data).
 */
SDK_API int32_t SDK_CALL sdk_get_user_sig(const char *data, size_t len,
                                          char **out_data, size_t *out_len);
/**
 * @brief Queries the Cookie snapshot history for one environment.
 *
 * This is a synchronous request to the backend `getCookieHistory` API.
 * The environment does not need to have a running browser process.
 *
 * Request body:
 * @code{.json}
 * {"envId":"2062428528552448000"}
 * @endcode
 *
 * @param data UTF-8 JSON request body. `envId` must be a decimal string.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated buffer containing the raw backend
 *        response JSON.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` on success; otherwise, a BroSDK error code.
 *
 * @note The SDK must be initialized and have valid backend credentials.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_get_cookies_history(const char *data, size_t len,
                                                 char **out_data,
                                                 size_t *out_len);

/**
 * @brief Reads the latest Cookie snapshot persisted in the local SQLite cache.
 *
 * The SDK decrypts the cached Cookie packet and returns a JSON array. This
 * operation does not require a running browser process. If the environment is
 * currently running, the returned value is still the latest SQLite snapshot,
 * not the browser's live Cookie state.
 *
 * Request body:
 * @code{.json}
 * {"envId":"2062428528552448000"}
 * @endcode
 *
 * Successful response:
 * @code{.json}
 * [{"name":"sessionid","value":"...","domain":".example.com"}]
 * @endcode
 *
 * @param data UTF-8 JSON request body. `envId` must be a decimal string.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated buffer containing a Cookie JSON
 *        array.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` on success, `CL_ENOTFOUND` when no local snapshot exists,
 *         or another BroSDK error code.
 *
 * @note The SDK must be initialized because the current environment key is
 *       required to decrypt the cached packet.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_get_cookies_local(const char *data, size_t len,
                                               char **out_data,
                                               size_t *out_len);

/**
 * @brief Replaces the Cookie snapshot in the local SQLite cache.
 *
 * The SDK validates and normalizes the input array, encrypts it with the
 * current environment key, and writes a new local Cookie revision. This
 * operation updates SQLite only; it does not modify a running browser process
 * and does not upload the snapshot to OSS.
 *
 * Request body:
 * @code{.json}
 * {"envId":"2062428528552448000","cookies":[]}
 * @endcode
 *
 * Successful response:
 * @code{.json}
 * {
 *   "envId":"2062428528552448000",
 *   "source":"sqlite",
 *   "cookieCount":0,
 *   "packetBytes":128,
 *   "revision":42,
 *   "syncState":"dirty",
 *   "changed":true,
 *   "disposition":"committed"
 * }
 * @endcode
 *
 * @param data UTF-8 JSON request body. `envId` must be a decimal string and
 *        `cookies` must be a JSON array.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated JSON write summary.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` on success; otherwise, a BroSDK error code.
 *
 * @warning A running browser may persist a newer live snapshot later and
 *          replace this local value when that browser closes.
 * @note The SDK must be initialized because the current environment key is
 *       required to encrypt the new packet.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_set_cookies_local(const char *data, size_t len,
                                               char **out_data,
                                               size_t *out_len);

/**
 * @brief Downloads and decrypts a Cookie snapshot from OSS.
 *
 * `fileUrl` is an OSS object key obtained from a Cookie history item. The SDK
 * verifies that the key belongs to the requested environment, downloads the
 * encrypted packet, optionally verifies its MD5 digest, and returns the
 * decoded Cookie JSON array. The operation does not update the local SQLite
 * cache or a running browser process.
 *
 * Request body:
 * @code{.json}
 * {
 *   "envId":"2078235798880129024",
 *   "fileUrl":"brosdk/apps/2059060776739540992/cookie/1784543535/2078235798880129024-v1.br",
 *   "md5":"0123456789abcdef0123456789abcdef"
 * }
 * @endcode
 *
 * Successful response:
 * @code{.json}
 * [{"name":"sessionid","value":"...","domain":".example.com"}]
 * @endcode
 *
 * @param data UTF-8 JSON request body. `envId` and `fileUrl` are required;
 *        `md5` is optional and, when present, must contain 32 hexadecimal
 *        characters.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated buffer containing a Cookie JSON
 *        array.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` on success; otherwise, a BroSDK error code.
 *
 * @note The SDK must be initialized and have valid backend and OSS
 *       credentials.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_get_cookies_remote(const char *data, size_t len,
                                                char **out_data,
                                                size_t *out_len);

/**
 * @brief Encrypts and uploads a Cookie snapshot without opening the browser.
 *
 * The SDK obtains the current environment metadata, validates and normalizes
 * the input array, encrypts the Cookie packet, uploads it to the environment's
 * OSS Cookie object, and updates backend metadata through `upCookie`. After a
 * successful remote update, the SDK also attempts to refresh its local SQLite
 * snapshot.
 *
 * Request body:
 * @code{.json}
 * {"envId":"2062428528552448000","cookies":[]}
 * @endcode
 *
 * @param data UTF-8 JSON request body. `envId` must be a decimal string and
 *        `cookies` must be a JSON array.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated buffer containing the raw backend
 *        `upCookie` response JSON, or an SDK success summary when the backend
 *        response body is empty.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` only after both the OSS upload and backend metadata update
 *         succeed; otherwise, a BroSDK error code.
 *
 * @warning This operation does not modify a running browser process. A later
 *          browser close may upload that browser's live Cookie state and
 *          replace this remote snapshot.
 * @note The SDK must be initialized and have valid backend and OSS
 *       credentials.
 * @note A local SQLite refresh failure does not roll back an otherwise
 *       successful remote update.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_set_cookies_remote(const char *data, size_t len,
                                                char **out_data,
                                                size_t *out_len);

/**
 * @brief Performs an offline health analysis of a Cookie JSON array.
 *
 * The analysis groups Cookies by normalized exact domain and reports
 * structural issues, session/persistent counts, expiration state, duplicate
 * identities, security attribute warnings, and authentication-token hints.
 * JWT-like values are inspected only for their `exp`, `nbf`, and `iat` time
 * claims. Signatures are not verified, and Cookie values are never included
 * in the response.
 *
 * Request body:
 * @code{.json}
 * {"cookies":[]}
 * @endcode
 *
 * Successful response shape:
 * @code{.json}
 * {
 *   "checkedAt":1753099200,
 *   "expiresSoonThresholdSeconds":86400,
 *   "summary":{"status":"warning","cookieCount":1,"domainCount":1},
 *   "domains":[{
 *     "domain":"example.com",
 *     "nextExpirationRemainingSeconds":3600,
 *     "cookies":[{
 *       "cookieName":"sid",
 *       "path":"/",
 *       "persistence":"persistent",
 *       "expiration":1753102800,
 *       "remainingSeconds":3600,
 *       "status":"expiring_soon",
 *       "authCandidate":true
 *     }]
 *   }],
 *   "globalIssues":[]
 * }
 * @endcode
 *
 * @param data UTF-8 JSON request body containing a required `cookies` array.
 * @param len Exact request size in bytes, excluding any trailing null byte.
 * @param out_data Receives an SDK-allocated health report JSON object.
 * @param out_len Receives the response size in bytes. The response is not
 *        guaranteed to be null-terminated.
 * @return `CL_OK` on success or `CL_EINVALID` when the request or Cookie array
 *         is invalid; otherwise, a BroSDK error code.
 *
 * @note This function is synchronous, performs no network or browser I/O, and
 *       does not require SDK initialization.
 * @note `expiresSoonThresholdSeconds` is only the warning window. A Cookie's
 *       signed lifetime is reported as `domains[].cookies[].remainingSeconds`;
 *       negative values are already expired, while session Cookies and missing
 *       expirations use JSON null. Domain-level earliest, latest, and next
 *       expiration fields have matching remaining-seconds fields.
 * @note A `time_valid` token status means only that the decoded JWT time window
 *       currently permits use. It does not prove signature validity or an
 *       active server-side login session.
 * @note Read `out_data` only when the function succeeds. Release every
 *       non-null output buffer with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_cookies_health_check(const char *data, size_t len,
                                                  char **out_data,
                                                  size_t *out_len);
/* Perform network diagnostics synchronously.
 * Request body is UTF-8 JSON:
 *   {"proxy":"","bridgeProxy":"","url":"https://baidu.com"}
 * Returns raw diagnostics JSON in *out_data. */
SDK_API int32_t SDK_CALL sdk_network_diagnostics(const char *data, size_t len,
                                                 char **out_data,
                                                 size_t *out_len);
/* Read current system proxy settings and the bridge-compatible upstream route.
 * Returns raw JSON in *out_data. */
SDK_API int32_t SDK_CALL sdk_system_proxy_diagnostics(char **out_data,
                                                      size_t *out_len);

/* Install browser core resources asynchronously.
 * Returns a reqId when accepted; progress/final results arrive via callback. */
SDK_API int32_t SDK_CALL sdk_browser_install(const char *data, size_t len);

/* Read the current running browser list synchronously.
 * Returns a raw JSON array in *out_data. */
SDK_API int32_t SDK_CALL sdk_browser_info(char **out_data, size_t *out_len);

/* Open one or more browser environments asynchronously.
 * Request body is UTF-8 JSON. Recommended shape:
 *   {"envs":[{"envId":"...","urls":["https://..."],"args":["--flag"]}]}
 * Returns a reqId when accepted.
 * Final success is browser-open-success, which implies CDP is ready.
 * If an environment is already running, the SDK activates its existing
 * browser window without relaunching it and reports browser-open-success with
 * the CL_WBRWALREADYRUNNING warning. The response data includes
 * `alreadyRunning: true` for this idempotent reuse. */
SDK_API int32_t SDK_CALL sdk_browser_open(const char *data, size_t len);

/* Close one or more browser environments asynchronously.
 * Request body is UTF-8 JSON. Recommended shape:
 *   {"envs":["envId1","envId2"]}
 * Returns a reqId when accepted.
 * Final completion is browser-close-success via callback / WebSocket. */
SDK_API int32_t SDK_CALL sdk_browser_close(const char *data, size_t len);

/* Synchronously remove local browser caches.
 * Request body:
 *   {"envs":["envId1","envId2"]}
 *   {"cores":[{"major":141}]}
 *   {"cores":[]}
 *   {"envs":["envId1"],"cores":[{"major":141}]}
 * envs removes user-data-dir caches for non-running environments.
 * cores removes browser core download cache files under cores/.cache:
 *   omitted cores = do not clean core cache; [] = clean all .cache files;
 *   [{"major":141}] = clean cache files for that browser major.
 * If any requested environment is running or in an open/close operation, that
 * environment is returned as busy and the function returns CL_EBUSY. */
SDK_API int32_t SDK_CALL sdk_browser_cleanup(const char *data, size_t len,
                                             char **out_data, size_t *out_len);
/* Send one raw CDP method/params command to a running browser environment.
 * Request body is UTF-8 JSON:
 *   {"envId":"...","method":"Runtime.evaluate","params":{},"sessionId":""}
 * envId must be a string. params is optional and defaults to {}.
 * sessionId is optional; when present the command is sent as a session command.
 * Returns the raw CDP response JSON in *out_data; caller calls sdk_free(). */
SDK_API int32_t SDK_CALL sdk_browser_command(const char *data, size_t len,
                                             char **out_data, size_t *out_len);
/* Open the built-in fingerprint check page in a running browser environment.
 * Request body is UTF-8 JSON:
 *   {"envId":"..."}
 * The page is released from embedded SDK resources into workDir/resources and
 * opened through CDP Target.createTarget as a new tab.
 * Returns the raw CDP response JSON in *out_data; caller calls sdk_free(). */
SDK_API int32_t SDK_CALL sdk_browser_env_check(const char *data, size_t len,
                                               char **out_data,
                                               size_t *out_len);
/* Capture page metadata, HTML and screenshots from a running browser.
 * Request body is UTF-8 JSON:
 *   {"envId":"...","includeHtml":true,"includeScreenshot":true}
 *   {"envId":"...","emitEvents":true}
 * Returns a JSON manifest plus chunk array in *out_data; caller calls
 * sdk_free(). When emitEvents=true, the same snapshot is additionally delivered
 * as new browser.snapshot.* callback/WebSocket events; existing browser
 * lifecycle events are not changed. */
SDK_API int32_t SDK_CALL sdk_browser_snapshot(const char *data, size_t len,
                                              char **out_data, size_t *out_len);
/* Create an environment synchronously.
 * The request body is forwarded to the backend env/create API.
 * The response body is the raw backend JSON and must be freed with sdk_free().
 */
SDK_API int32_t SDK_CALL sdk_env_create(const char *data, size_t len,
                                        char **out_data, size_t *out_len);

/* Update an environment synchronously.
 * Request/response semantics match sdk_env_create: backend JSON passthrough. */
SDK_API int32_t SDK_CALL sdk_env_update(const char *data, size_t len,
                                        char **out_data, size_t *out_len);

/* Query environments synchronously.
 * Request/response semantics are backend JSON passthrough. */
SDK_API int32_t SDK_CALL sdk_env_page(const char *data, size_t len,
                                      char **out_data, size_t *out_len);

/* Request one environment's backend getEnvInfo payload synchronously.
 * Request/response semantics are backend JSON passthrough. */
SDK_API int32_t SDK_CALL sdk_env_getinfo(const char *data, size_t len,
                                         char **out_data, size_t *out_len);

/* Destroy an environment synchronously.
 * Request/response semantics are backend JSON passthrough. */
SDK_API int32_t SDK_CALL sdk_env_destroy(const char *data, size_t len,
                                         char **out_data, size_t *out_len);

SDK_API int32_t SDK_CALL sdk_env_get_cookies(const char *, size_t len);

/* Refresh userSig asynchronously.
 * Returns a reqId when accepted; final result is reported via callback. */
SDK_API int32_t SDK_CALL sdk_token_update(const char *data, size_t len);

/* Synchronously stop the SDK and destroy the singleton.
 * Recommended after all browser-close-success events have been received. */
SDK_API int32_t SDK_CALL sdk_shutdown(void);

/* Shared heap helpers.
 * Use sdk_free() to release any buffer returned by the SDK. */
SDK_API void SDK_CALL sdk_free(void *ptr);
SDK_API void *SDK_CALL sdk_malloc(size_t size);

/* Static error/event string helpers. Returned pointers must not be freed. */
SDK_API const char *SDK_CALL sdk_error_name(int32_t code);
SDK_API const char *SDK_CALL sdk_error_string(int32_t code);
SDK_API const char *SDK_CALL sdk_event_name(int32_t evtid);

/* Status classification helpers. */
SDK_API bool SDK_CALL sdk_is_error(int32_t code);
SDK_API bool SDK_CALL sdk_is_warn(int32_t code);
SDK_API bool SDK_CALL sdk_is_reqid(int32_t code);
SDK_API bool SDK_CALL sdk_is_ok(int32_t code);
SDK_API bool SDK_CALL sdk_is_done(int32_t code);
SDK_API bool SDK_CALL sdk_is_event(int32_t code);

#ifdef __cplusplus
}
#endif

/* C++ interface (only visible to C++ callers).
 *
 * Usage:
 * - call sdk_init(...) and cast sdk_handle_t to ISDK*
 * - or call sdk_init_cpp(...) to obtain the current singleton as ISDK*
 *
 * Semantics are intentionally aligned with the C API:
 * - sync methods return their result immediately and may fill malloc'd output
 * - async methods usually return CL_DONE when the task is accepted
 * - async final results still flow through sdk_result_cb / WebSocket
 * - any output buffer returned here must be released with sdk_free() */
#ifdef __cplusplus
class ISDK {
public:
  virtual ~ISDK() = default;

  /* ── Lifecycle ─────────────────────────────────────────────────────────── */
  /* Synchronous initialization.
   * Equivalent to sdk_init(...).
   *
   * data/len  : UTF-8 JSON init request. Common fields include:
   *             userSig(required), workDir(optional), port(optional),
   *             sdkApiUrl(optional), debug(optional).
   * out/out_len:
   *             malloc'd UTF-8 JSON response; caller must call sdk_free(*out).
   *
   * Returns CL_OK on success or a negative error code on failure. */
  virtual int32_t Init(const char *data, size_t len, char **out,
                       size_t *out_len) const = 0;
  /* Asynchronous initialization.
   * Equivalent to sdk_init_async(...).
   *
   * Returns a reqId when the request is accepted.
   * Final success/failure is reported via sdk_result_cb or WebSocket push. */
  virtual int32_t Init(const char *data, size_t len) const = 0;

  /* Synchronously stop the SDK and destroy the singleton.
   * Equivalent to sdk_shutdown().
   *
   * Recommended after all browser-close-success events have been received. */
  virtual int32_t Shutdown() const = 0;

  /* Asynchronously refresh userSig.
   * Equivalent to sdk_token_update(...).
   *
   * data/len  : UTF-8 JSON body; typically carries a new "userSig".
   * Returns CL_DONE when the refresh request is accepted.
   * Final success/failure is reported via sdk_result_cb or WebSocket push. */
  virtual int32_t UpdateToken(const char *data, size_t len) const = 0;

  /* ── SDK / browser info (malloc'd; caller calls sdk_free) ─────────────── */
  /* Get a userSig synchronously.
   * Equivalent to sdk_get_user_sig(...).
   *
   * data/len  : UTF-8 JSON body; apiKey is required.
   * On success returns the raw backend getUserSig JSON in *out.
   * Caller must release *out with sdk_free(). */
  virtual int32_t GetUserSig(const char *data, size_t len, char **out,
                             size_t *out_len) const = 0;

  /* Read SDK runtime information synchronously.
   * Equivalent to sdk_info().
   *
   * On success returns raw info JSON in *out, not the async callback-style
   * envelope. Caller must release *out with sdk_free(). */
  virtual int32_t Info(char **out, size_t *out_len) const = 0;

  /* Perform network diagnostics synchronously.
   *
   * data/len  : UTF-8 JSON body; {"proxy":"","bridgeProxy":"","url":""}.
   * On success returns raw diagnostics JSON in *out, not the async
   * callback-style envelope. Caller must release *out with sdk_free(). */
  virtual int32_t NetworkDiagnostics(const char *data, size_t len, char **out,
                                     size_t *out_len) const = 0;

  /* Read current system proxy settings and bridge-compatible upstream route.
   * Equivalent to sdk_system_proxy_diagnostics(...). */
  virtual int32_t SystemProxyDiagnostics(char **out, size_t *out_len) const = 0;

  /* Read the current running browser list synchronously.
   * Equivalent to sdk_browser_info().
   *
   * On success returns a raw JSON array in *out.
   * Caller must release *out with sdk_free(). */
  virtual int32_t BrowserInfo(char **out, size_t *out_len) const = 0;

  /* ── Browser install / open / close ────────────────────────────────────── */
  /* Asynchronously install browser core resources.
   * Equivalent to sdk_browser_install(...).
   *
   * Returns a reqId when accepted; progress/final results arrive via the
   * global async callback or WebSocket push. */
  virtual int32_t BrowserInstall(const char *data, size_t len) const = 0;

  /* Asynchronously open one or more browser environments.
   * Equivalent to sdk_browser_open(...).
   *
   * Recommended request shape:
   *   {"envs":[{"envId":"...","urls":["https://..."],"args":["--flag"]}]}
   *
   * Returns a reqId when accepted.
   * The true ready signal is browser-open-success, which implies CDP is ready.
   * An already-running environment is activated and also completes with
   * browser-open-success plus CL_WBRWALREADYRUNNING; it is not relaunched.
   */
  virtual int32_t BrowserOpen(const char *data, size_t len) const = 0;

  /* Asynchronously close one or more browser environments.
   * Equivalent to sdk_browser_close(...).
   *
   * Recommended request shape:
   *   {"envs":["envId1","envId2"]}
   *
   * Returns a reqId when accepted.
   * The real close completion signal is browser-close-success. */
  virtual int32_t BrowserClose(const char *data, size_t len) const = 0;

  /* Synchronously remove local browser caches.
   * Equivalent to sdk_browser_cleanup(...).
   *
   * Request body:
   *   {"envs":["envId1","envId2"]}
   *   {"cores":[{"major":141}]}
   *   {"cores":[]}
   *   {"envs":["envId1"],"cores":[{"major":141}]}
   *
   * envs removes user-data-dir caches for non-running environments. cores
   * removes browser core download cache files under cores/.cache. If any
   * requested env is currently running or in a browser open/close operation,
   * that env is reported as busy and the function returns CL_EBUSY. */
  virtual int32_t BrowserCleanup(const char *data, size_t len, char **out,
                                 size_t *out_len) const = 0;

  /* Send one CDP command to a running browser environment synchronously.
   * Equivalent to sdk_browser_command(...). */
  virtual int32_t BrowserCommand(const char *data, size_t len, char **out,
                                 size_t *out_len) const = 0;
  /* Open the built-in fingerprint check page in a running browser environment.
   * Equivalent to sdk_browser_env_check(...). */
  virtual int32_t BrowserEnvCheck(const char *data, size_t len, char **out,
                                  size_t *out_len) const = 0;

  /* ── Environment CRUD (sync; fills *out / *out_len, caller calls sdk_free) */
  /* Create an environment synchronously.
   * Equivalent to sdk_env_create(...).
   *
   * The request body is forwarded to the backend env/create API.
   * On success *out contains the raw backend JSON response. */
  virtual int32_t CreateEnv(const char *data, size_t len, char **out,
                            size_t *out_len) const = 0;

  /* Update an environment synchronously.
   * Equivalent to sdk_env_update(...).
   *
   * The request body is forwarded to the backend env/update API.
   * On success *out contains the raw backend JSON response. */
  virtual int32_t UpdateEnv(const char *data, size_t len, char **out,
                            size_t *out_len) const = 0;

  /* Query environments synchronously.
   * Equivalent to sdk_env_page(...).
   *
   * The request body is forwarded to the backend env/page API.
   * On success *out contains the raw backend JSON response. */
  virtual int32_t PageEnv(const char *data, size_t len, char **out,
                          size_t *out_len) const = 0;

  /* Request one environment's getEnvInfo payload synchronously.
   * Equivalent to sdk_env_getinfo(...).
   *
   * The request body is forwarded to the backend getEnvInfo API.
   * On success *out contains the raw backend JSON response. */
  virtual int32_t GetEnvInfo(const char *data, size_t len, char **out,
                             size_t *out_len) const = 0;

  /* Destroy an environment synchronously.
   * Equivalent to sdk_env_destroy(...).
   *
   * The request body is forwarded to the backend env/destroy API.
   * On success *out contains the raw backend JSON response. */
  virtual int32_t DestroyEnv(const char *data, size_t len, char **out,
                             size_t *out_len) const = 0;

  /* ── Callbacks (not const — they mutate internal state) ───────────────────
   */
  /* Register the global async result callback. Equivalent to
   * sdk_register_result_cb(...). Pass nullptr to disable. */
  virtual int32_t RegisterResultCb(sdk_result_cb_t cb, void *user_data) = 0;

  /* Register the SDK log callback.
   * Equivalent to sdk_register_log_cb(...).
   *
   * Pass nullptr to disable log callback delivery. */
  virtual int32_t RegisterLogCb(sdk_log_cb_t cb) = 0;

  /* Register the cookie persistence interception callback. Equivalent to
   * sdk_register_cookies_storage_cb(...). Replacement buffers must use
   * sdk_malloc(); pass nullptr to disable interception. */
  virtual int32_t RegisterCookiesStorageCb(sdk_cookies_storage_cb_t cb,
                                           void *user_data) = 0;
  /* Register the security strategy decision callback. Equivalent to
   * sdk_register_security_decision_cb(...). Pass nullptr to disable. */
  virtual int32_t RegisterSecurityDecisionCb(sdk_security_decision_cb_t cb,
                                             void *user_data) = 0;

  /**
   * @brief C++ equivalent of sdk_get_cookies_history().
   * @see sdk_get_cookies_history()
   */
  virtual int32_t GetCookieHistory(const char *data, size_t len, char **out,
                                   size_t *out_len) const = 0;

  /**
   * @brief C++ equivalent of sdk_get_cookies_local().
   * @see sdk_get_cookies_local()
   */
  virtual int32_t GetCookiesLocal(const char *data, size_t len, char **out,
                                  size_t *out_len) const = 0;

  /**
   * @brief C++ equivalent of sdk_set_cookies_local().
   * @see sdk_set_cookies_local()
   */
  virtual int32_t SetCookiesLocal(const char *data, size_t len, char **out,
                                  size_t *out_len) const = 0;

  /**
   * @brief C++ equivalent of sdk_get_cookies_remote().
   * @see sdk_get_cookies_remote()
   */
  virtual int32_t GetCookiesRemote(const char *data, size_t len, char **out,
                                   size_t *out_len) const = 0;

  /**
   * @brief C++ equivalent of sdk_set_cookies_remote().
   * @see sdk_set_cookies_remote()
   *
   * @note These virtual methods are appended to preserve existing ISDK vtable
   *       slots.
   */
  virtual int32_t SetCookiesRemote(const char *data, size_t len, char **out,
                                   size_t *out_len) const = 0;

  /**
   * @brief C++ equivalent of sdk_cookies_health_check().
   * @see sdk_cookies_health_check()
   *
   * @note This virtual method is appended to preserve existing ISDK vtable
   *       slots.
   */
  virtual int32_t CheckCookiesHealth(const char *data, size_t len, char **out,
                                     size_t *out_len) const = 0;
};
#endif

#endif ///__BROSDK_H__

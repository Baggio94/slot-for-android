# SleepManager status indicator interoperability

SleepManager 0.7.3 and later expose a read-only status API at
`content://com.med.sleepmanager.status` using
`ContentResolver.call(uri, "status", null, null)`.

Response key `status` is a JSON object string, for example:

```json
{"apiVersion":1,"enabled":true,"serviceRunning":true}
```

Slot displays the SleepManager status indicator as
**running** only when both `enabled` and `serviceRunning` are true.
A configured-on but non-running service displays stopped (it is not operating).
Apps installed without the v1 provider, including stable SleepManager 0.7.2,
retain the previous **best-effort** Android 12 service enumeration fallback.
Unknown API responses never silently disable SleepManager.

The status provider is read-only and cannot toggle the manager. Slot listens
for `ContentObserver` notifications while the shelf is visible/active,
resumes with a fresh status query and executes provider calls on its existing
low-priority worker. No synchronous IPC is done on the UI/GL render thread.
The integration does not require a foreground service or a control API.

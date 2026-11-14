package fyi.slot.konkr

import android.app.admin.DeviceAdminReceiver

/** "Lock the screen" only: a closed RG Rotate goes to sleep (see LidCover). */
class SlotDeviceAdmin : DeviceAdminReceiver()

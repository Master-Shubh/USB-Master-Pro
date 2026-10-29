/**
 * USB Master Pro — Upper Filter Driver for Physical Hardware Capture (anyusb_filter.sys)
 * KMDF 1.33 Driver Interface Header
 * Target: Windows 10 & Windows 11 (x64 / ARM64)
 */

#ifndef _ANYUSB_FILTER_H_
#define _ANYUSB_FILTER_H_

#include <ntddk.h>
#include <wdf.h>
#include <usb.h>
#include <usbioctl.h>
#include <initguid.h>
#include <devpkey.h>

#define ANYUSB_FILTER_POOL_TAG 'TFNA' // 'ANTF'

// Custom IOCTLs for User-Mode Daemon Communication
#define FILE_DEVICE_ANYUSB_FILTER 0x8901
#define IOCTL_ANYUSB_FILTER_START_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x810, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_ANYUSB_FILTER_STOP_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x811, METHOD_BUFFERED, FILE_ANY_ACCESS)

typedef struct _FILTER_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFIOTARGET             IoTarget;
    WDFQUEUE                IoctlQueue;
    BOOLEAN                 IsCaptured;
    ULONG                   SessionId;
} FILTER_DEVICE_DATA, *PFILTER_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(FILTER_DEVICE_DATA, GetFilterData)

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD AnyUsbFilter_EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbFilter_EvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL AnyUsbFilter_EvtIoInternalDeviceControl;
EVT_WDF_REQUEST_COMPLETION_ROUTINE AnyUsbFilter_EvtUrbComplete;

#endif // _ANYUSB_FILTER_H_

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
#include <ntstrsafe.h>

// KMDF Compatibility Macro: Retrieve WDFDEVICE from WDFREQUEST via its WDFQUEUE
#ifndef WdfRequestGetDevice
#define WdfRequestGetDevice(Request) (WdfIoQueueGetDevice(WdfRequestGetIoQueue(Request)))
#endif

#define ANYUSB_FILTER_POOL_TAG 'TFNA' // 'ANTF'

// Custom IOCTLs for User-Mode Daemon Communication
#define FILE_DEVICE_ANYUSB_FILTER 0x8901
#define IOCTL_ANYUSB_FILTER_START_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x810, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_ANYUSB_FILTER_STOP_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x811, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_ANYUSB_FILTER_FETCH_URB \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x812, METHOD_OUT_DIRECT, FILE_ANY_ACCESS)
#define IOCTL_ANYUSB_FILTER_COMPLETE_URB \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x813, METHOD_IN_DIRECT, FILE_ANY_ACCESS)

// Maximum single-URB buffer transfer size (64 KB).
#define ANYUSB_MAX_URB_BUFFER_SIZE (64 * 1024)
#define ANYUSB_MAX_PENDING_URBS 256

#pragma pack(push, 1)

// Payload for starting or stopping capture on a target device
typedef struct _ANYUSB_FILTER_CAPTURE_REQUEST {
    ULONG       DeviceId;
} ANYUSB_FILTER_CAPTURE_REQUEST, *PANYUSB_FILTER_CAPTURE_REQUEST;

// Payload for completing an intercepted URB returned from user-mode daemon
typedef struct _ANYUSB_COMPLETE_REQUEST {
    ULONG       DeviceId;
    ULONG64     SequenceId;
    NTSTATUS    Status;
    ULONG       ActualLength;
    UCHAR       Data[ANYUSB_MAX_URB_BUFFER_SIZE];
} ANYUSB_COMPLETE_REQUEST, *PANYUSB_COMPLETE_REQUEST;

// Payload for fetching an intercepted URB to daemon
typedef struct _ANYUSB_URB_MESSAGE {
    ULONG       DeviceId;
    ULONG64     SequenceId;
    UCHAR       EndpointAddress;
    UCHAR       TransferType; // 0=Control, 1=Isoch, 2=Bulk, 3=Interrupt
    ULONG       TransferBufferLength;
    UCHAR       SetupPacket[8];
    UCHAR       Data[ANYUSB_MAX_URB_BUFFER_SIZE];
} ANYUSB_URB_MESSAGE, *PANYUSB_URB_MESSAGE;

#pragma pack(pop)

// Pending URB tracking slot with sequence token & device binding to prevent ABA stale mismatches
typedef struct _ANYUSB_PENDING_SLOT {
    WDFREQUEST  Request;
    ULONG64     SequenceId;
    ULONG       DeviceId;
} ANYUSB_PENDING_SLOT, *PANYUSB_PENDING_SLOT;

// Control Device Object (CDO) Context for user-mode daemon interface (\Device\AnyUsbFilter)
typedef struct _CONTROL_DEVICE_DATA {
    WDFDEVICE               ControlDevice;
    WDFQUEUE                IoctlQueue;
    WDFQUEUE                PendingUrbQueue; // Holds intercepted URBs for daemon extraction
    WDFSPINLOCK             PendingUrbLock;  // Protects PendingRequests table
    ANYUSB_PENDING_SLOT     PendingRequests[ANYUSB_MAX_PENDING_URBS];
    WDFSPINLOCK             ControlLock;
    LIST_ENTRY              FilterDeviceList;
    BOOLEAN                 IsCaptured;
} CONTROL_DEVICE_DATA, *PCONTROL_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(CONTROL_DEVICE_DATA, GetControlData)

// FiDO Context for each filtered physical USB device stack
typedef struct _FILTER_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFIOTARGET             IoTarget;
    WDFQUEUE                IoctlQueue;
    BOOLEAN                 IsCaptured;
    ULONG                   SessionId;
    ULONG                   DeviceId;
    LIST_ENTRY              Link;
} FILTER_DEVICE_DATA, *PFILTER_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(FILTER_DEVICE_DATA, GetFilterData)

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD AnyUsbFilter_EvtDeviceAdd;
EVT_WDF_DRIVER_UNLOAD AnyUsbFilter_EvtDriverUnload;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbFilter_EvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbFilter_EvtControlIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL AnyUsbFilter_EvtIoInternalDeviceControl;
EVT_WDF_REQUEST_COMPLETION_ROUTINE AnyUsbFilter_EvtUrbComplete;
EVT_WDF_REQUEST_CANCEL AnyUsbFilter_EvtPendingUrbCancel;
EVT_WDF_OBJECT_CONTEXT_CLEANUP AnyUsbFilter_EvtDeviceCleanup;
EVT_WDF_FILE_CLEANUP AnyUsbFilter_EvtControlFileCleanup;

VOID
AnyUsbFilter_DrainPendingRequests(
    _In_ PCONTROL_DEVICE_DATA ControlData,
    _In_ ULONG                TargetDeviceId,
    _In_ NTSTATUS             CompletionStatus
);

#endif // _ANYUSB_FILTER_H_

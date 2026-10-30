/**
 * USB Master Pro — Virtual USB Host Controller & Root Bus Driver (VHCI)
 * KMDF 1.33 Driver Interface Header
 * Target: Windows 10 & Windows 11 (x64 / ARM64)
 */

#ifndef _ANYUSB_VHCI_H_
#define _ANYUSB_VHCI_H_

#include <ntddk.h>
#include <wdf.h>
#include <usb.h>
#include <usbioctl.h>
#include <usbbusif.h>
#include <ntstrsafe.h>

// KMDF Compatibility Macro: Retrieve WDFDEVICE from WDFREQUEST via its WDFQUEUE
#ifndef WdfRequestGetDevice
#define WdfRequestGetDevice(Request) (WdfIoQueueGetDevice(WdfRequestGetIoQueue(Request)))
#endif

#define ANYUSB_VHCI_POOL_TAG 'ICHV' // 'VHCI'

// Custom IOCTLs for User-Mode Daemon Communication (DevType 0x8900)
#define FILE_DEVICE_ANYUSB_VHCI 0x8900

#define IOCTL_ANYUSB_VHCI_INIT_BUS \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_ANYUSB_VHCI_ATTACH_DEVICE \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_ANYUSB_VHCI_DETACH_DEVICE \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_ANYUSB_VHCI_FETCH_PENDING_URB \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x804, METHOD_OUT_DIRECT, FILE_ANY_ACCESS)

#define IOCTL_ANYUSB_VHCI_COMPLETE_URB \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x805, METHOD_IN_DIRECT, FILE_ANY_ACCESS)

#define IOCTL_ANYUSB_VHCI_ABORT_PIPE \
    CTL_CODE(FILE_DEVICE_ANYUSB_VHCI, 0x806, METHOD_BUFFERED, FILE_ANY_ACCESS)

#pragma pack(push, 1)

// Information payload passed by daemon when attaching a remote USB device
typedef struct _ANYUSB_ATTACH_REQUEST {
    ULONG       DeviceId;            // Unique session device ID
    USHORT      VendorId;            // e.g. 0x27D0 (Mantra Softech)
    USHORT      ProductId;           // e.g. 0x0041 (MFS100)
    USHORT      BcdDevice;           // Device revision e.g. 0x0100
    UCHAR       DeviceClass;         // e.g. 0x00 or 0x0E (Video)
    UCHAR       DeviceSubClass;
    UCHAR       DeviceProtocol;
    UCHAR       Speed;               // 1 = Low, 2 = Full, 3 = High, 4 = SuperSpeed
    WCHAR       SerialNumber[64];    // Hardware serial string
    WCHAR       Manufacturer[64];    // Manufacturer string
    WCHAR       ProductDescription[128]; // Product string
    ULONG       RawDescriptorLength; // Raw USB configuration descriptor blob
    UCHAR       RawDescriptors[1024];
} ANYUSB_ATTACH_REQUEST, *PANYUSB_ATTACH_REQUEST;

// Payload for detaching a remote USB device
typedef struct _ANYUSB_DETACH_REQUEST {
    ULONG       DeviceId;            // Session device ID to detach
} ANYUSB_DETACH_REQUEST, *PANYUSB_DETACH_REQUEST;

// Maximum single-URB buffer transfer size (64 KB).
// NOTE: Single URBs exceeding this ceiling (e.g. uncompressed 2.5 MB slap biometrics or video frames)
// are strictly REJECTED with STATUS_BUFFER_OVERFLOW to prevent silent data corruption.
// Transfers exceeding 64 KB must be chunked across multiple URBs.
#define ANYUSB_MAX_URB_BUFFER_SIZE (64 * 1024)

// Device Interface GUID for PnP enumeration: {A83F122B-5E09-4E65-B6D1-0428B31289A1}
DEFINE_GUID(GUID_DEVINTERFACE_ANYUSB_VHCI,
    0xa83f122b, 0x5e09, 0x4e65, 0xb6, 0xd1, 0x04, 0x28, 0xb3, 0x12, 0x89, 0xa1);

// Payload for completing a URB returned from remote peer
typedef struct _ANYUSB_COMPLETE_REQUEST {
    ULONG       DeviceId;
    ULONG64     SequenceId;
    NTSTATUS    Status;
    ULONG       ActualLength;
    UCHAR       Data[ANYUSB_MAX_URB_BUFFER_SIZE];
} ANYUSB_COMPLETE_REQUEST, *PANYUSB_COMPLETE_REQUEST;

// Payload for fetching a pending URB to daemon
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

#define ANYUSB_MAX_PENDING_URBS 256

// Pending URB tracking slot with sequence token & device binding to prevent ABA stale mismatches
typedef struct _ANYUSB_PENDING_SLOT {
    WDFREQUEST  Request;
    ULONG64     SequenceId;
    ULONG       DeviceId;
} ANYUSB_PENDING_SLOT, *PANYUSB_PENDING_SLOT;

// Bus FDO Device Context
typedef struct _FDO_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFQUEUE                IoctlQueue;
    WDFQUEUE                PendingUrbQueue; // Holds in-flight URBs from guest class drivers
    ULONG                   AttachedDeviceCount;
    WDFSPINLOCK             DeviceListLock;
    LIST_ENTRY              ChildPdoList;
    WDFSPINLOCK             PendingUrbLock;  // Protects the pending requests sequence table
    ANYUSB_PENDING_SLOT     PendingRequests[ANYUSB_MAX_PENDING_URBS];
} FDO_DEVICE_DATA, *PFDO_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(FDO_DEVICE_DATA, GetFdoData)

// Child PDO (Virtual Device) Context
typedef struct _PDO_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFDEVICE               ParentFdo;
    ULONG                   DeviceId;
    USHORT                  VendorId;
    USHORT                  ProductId;
    USHORT                  BcdDevice;
    UCHAR                   Speed;
    WCHAR                   HardwareId[128];
    WCHAR                   CompatibleIds[256];
    WCHAR                   DeviceDescription[128];
    LIST_ENTRY              Link;
    USB_BUS_INTERFACE_USBDI_V2 BusInterface;
} PDO_DEVICE_DATA, *PPDO_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(PDO_DEVICE_DATA, GetPdoData)

#endif // _ANYUSB_VHCI_H_

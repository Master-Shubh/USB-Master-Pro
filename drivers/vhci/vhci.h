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

// Payload for completing a URB returned from remote peer
typedef struct _ANYUSB_COMPLETE_REQUEST {
    ULONG       DeviceId;
    ULONG64     SequenceId;
    NTSTATUS    Status;
    ULONG       ActualLength;
} ANYUSB_COMPLETE_REQUEST, *PANYUSB_COMPLETE_REQUEST;

#pragma pack(pop)

// Bus FDO Device Context
typedef struct _FDO_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFQUEUE                IoctlQueue;
    WDFQUEUE                PendingUrbQueue; // Holds in-flight URBs from guest class drivers
    ULONG                   AttachedDeviceCount;
    WDFSPINLOCK             DeviceListLock;
    LIST_ENTRY              ChildPdoList;
} FDO_DEVICE_DATA, *PFDO_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(FDO_DEVICE_DATA, GetFdoData)

// Child PDO (Virtual Device) Context
typedef struct _PDO_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    ULONG                   DeviceId;
    USHORT                  VendorId;
    USHORT                  ProductId;
    USHORT                  BcdDevice;
    UCHAR                   Speed;
    WCHAR                   HardwareId[128];
    WCHAR                   CompatibleIds[256];
    WCHAR                   DeviceDescription[128];
    LIST_ENTRY              Link;
} PDO_DEVICE_DATA, *PPDO_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(PDO_DEVICE_DATA, GetPdoData)

#endif // _ANYUSB_VHCI_H_

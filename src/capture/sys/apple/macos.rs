use objc2::rc::Retained;
use objc2::{define_class, msg_send, AllocAnyThread, ClassType, DefinedClass, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use crate::capture::{CaptureData, CaptureHandler, CaptureKind};

pub type HandlerInner = Box<dyn CaptureHandler>;

pub fn register_handler<T>(handler: T)
where
    T: CaptureHandler,
{
    let mtm: MainThreadMarker =
        MainThreadMarker::new().expect("Application must be initialized on the main thread");

    let application = NSApplication::sharedApplication(mtm);

    let service_provider = ServiceProvider::new(Box::from(handler));
    unsafe {
        application.setServicesProvider(Some(service_provider.as_super()));
    }
}

struct ServiceProviderIvars {
    handler: HandlerInner,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MolyCaptureServiceProvider"]
    #[ivars = ServiceProviderIvars]
    struct ServiceProvider;

    unsafe impl NSObjectProtocol for ServiceProvider {}

    impl ServiceProvider {
        #[unsafe(method(capture:userData:error:))]
        fn capture(
            &self,
            pasteboard: &NSPasteboard,
            _user_data: Option<&NSString>,
            _error: *mut *mut NSString,
        ) {
            let contents;
            unsafe {
                contents = pasteboard.stringForType(&NSPasteboardTypeString);
            }

            if let Some(contents) = contents {
                let data = CaptureData::new(contents.to_string(), CaptureKind::Text);
                self.ivars().handler.capture(data);
            }
        }
    }
);

impl ServiceProvider {
    fn new(handler: HandlerInner) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ServiceProviderIvars { handler });
        unsafe { msg_send![super(this), init] }
    }
}

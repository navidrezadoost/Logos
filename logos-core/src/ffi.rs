use std::sync::Arc;
use std::ffi::{CString, c_char};
use crate::{Document, RectLayer, Layer};

/// Opaque handle type for C FFI
#[repr(C)]
pub struct LogosDocument {
    _private: [u8; 0],  // Prevent direct construction
}

/// Convert between Arc<Document> and FFI handle
fn into_ffi_handle(arc: Arc<Document>) -> *mut LogosDocument {
    let arc_ptr: *mut Arc<Document> = Box::into_raw(Box::new(arc));
    arc_ptr as *mut LogosDocument
}

/// SAFETY: Caller must ensure ptr is valid and not used after free
unsafe fn arc_from_ffi(ptr: *mut LogosDocument) -> Option<Arc<Document>> {
    if ptr.is_null() {
        return None;
    }
    let arc_ptr = ptr as *mut Arc<Document>;
    Some((*arc_ptr).clone()) // Clones the Arc, increasing ref count
}

#[no_mangle]
pub extern "C" fn logos_document_new() -> *mut LogosDocument {
    into_ffi_handle(Arc::new(Document::new()))
}

#[no_mangle]
pub extern "C" fn logos_document_free(ptr: *mut LogosDocument) {
    if !ptr.is_null() {
        unsafe {
            // Reconstruct the Box to drop it, which decrements the Arc count
            let _ = Box::from_raw(ptr as *mut Arc<Document>);
        }
    }
}

#[no_mangle]
pub extern "C" fn logos_document_add_rect(
    doc: *mut LogosDocument,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    error_out: *mut *mut c_char,
) -> i32 {
    let doc_arc = unsafe { arc_from_ffi(doc) };
    
    let Some(doc) = doc_arc else {
        if !error_out.is_null() {
             let error_msg = CString::new("Null document pointer").unwrap();
             unsafe { *error_out = error_msg.into_raw() };
        }
        return -1;
    };
    
    let rect = RectLayer::new(x, y, width, height);
    
    match doc.add_layer(Layer::Rect(rect)) {
        Ok(_) => 0,
        Err(e) => {
            if !error_out.is_null() {
                let error_msg = CString::new(e).unwrap_or_default();
                unsafe { *error_out = error_msg.into_raw() };
            }
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::ptr; // Imported here for tests

    #[test]
    fn test_ffi_lifecycle() {
        let doc = logos_document_new();
        assert!(!doc.is_null());
        logos_document_free(doc);
    }

    #[test]
    fn test_add_rect() {
        let doc = logos_document_new();
        let mut err_ptr: *mut c_char = ptr::null_mut();
        
        let result = logos_document_add_rect(doc, 10.0, 10.0, 100.0, 100.0, &mut err_ptr);
        assert_eq!(result, 0);
        assert!(err_ptr.is_null());
        
        logos_document_free(doc);
    }

    #[test]
    fn test_null_pointer_handling() {
        let mut err_ptr: *mut c_char = ptr::null_mut();
        let result = logos_document_add_rect(ptr::null_mut(), 0.0, 0.0, 0.0, 0.0, &mut err_ptr);
        assert_eq!(result, -1);
        
        assert!(!err_ptr.is_null());
        unsafe {
            let err_str = CStr::from_ptr(err_ptr).to_str().unwrap();
            assert_eq!(err_str, "Null document pointer");
            // In real C code, we'd need to free this string. 
            // Here, we just let it leak for the test or manually free it using CString::from_raw
            let _ = CString::from_raw(err_ptr); 
        }
    }
}

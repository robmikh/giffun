use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};

use windows::{
    core::{IInspectable, Result},
    Foundation::{TimeSpan, TypedEventHandler},
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
};

pub struct CaptureFrameGenerator {
    _d3d_device: IDirect3DDevice,
    _item: GraphicsCaptureItem,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    sender: Sender<Option<Direct3D11CaptureFrame>>,
    receiver: Receiver<Option<Direct3D11CaptureFrame>>,
}

unsafe impl Send for CaptureFrameGenerator {}
impl CaptureFrameGenerator {
    pub fn new(
        d3d_device: IDirect3DDevice,
        item: GraphicsCaptureItem,
        size: SizeInt32,
        number_of_buffers: u32,
    ) -> Result<Self> {
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &d3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            number_of_buffers as i32,
            size,
        )?;
        let session = frame_pool.CreateCaptureSession(&item)?;

        let (sender, receiver) = channel();
        frame_pool.FrameArrived(
            TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
                let session = session.clone();
                let sender = sender.clone();
                let mut last_timestamp: Option<TimeSpan> = None;
                move |frame_pool, _| {
                    let frame_pool = frame_pool.as_ref().unwrap();
                    let frame = frame_pool.TryGetNextFrame()?;
                    let timestamp = frame.SystemRelativeTime()?;
                    let mut update = true;
                    if let Some(last_timestamp) = &last_timestamp {
                        let time_since_last_update = TimeSpan {
                            Duration: timestamp.Duration - last_timestamp.Duration,
                        };
                        let duration: Duration = time_since_last_update.into();
                        if duration.as_millis() < 33 {
                            update = false;
                        }
                    }

                    if update {
                        last_timestamp = Some(timestamp);
                        if sender.send(Some(frame)).is_err() {
                            frame_pool.Close()?;
                            session.Close()?;
                        }
                    }
                    Ok(())
                }
            }),
        )?;

        Ok(Self {
            _d3d_device: d3d_device,
            _item: item,
            frame_pool,
            session,
            sender,
            receiver,
        })
    }

    pub fn session(&self) -> CaptureFrameGeneratorSession {
        CaptureFrameGeneratorSession {
            session: self.session.clone(),
            sender: self.sender.clone(),
        }
    }

    pub fn wait_for_next_frame(&mut self) -> Result<Option<Direct3D11CaptureFrame>> {
        if let Some(frame) = self.receiver.recv().unwrap() {
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }

    pub fn try_get_next_frame(&mut self) -> Result<Option<Direct3D11CaptureFrame>> {
        let result = self.receiver.try_recv();
        if let Ok(result) = result {
            if let Some(frame) = result {
                Ok(Some(frame))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}

impl Drop for CaptureFrameGenerator {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        self.session.Close().unwrap();
        self.frame_pool.Close().unwrap();
    }
}

pub struct CaptureFrameGeneratorSession {
    session: GraphicsCaptureSession,
    sender: Sender<Option<Direct3D11CaptureFrame>>,
}

impl CaptureFrameGeneratorSession {
    pub fn start(&self) -> Result<()> {
        self.session.StartCapture()
    }

    pub fn stop(self) -> Result<()> {
        self.session.Close()?;
        let _ = self.sender.send(None);
        Ok(())
    }
}

use wgpu::{
    COPY_BUFFER_ALIGNMENT, BufferDescriptor, BufferAddress, BufferUsages, Buffer, Device, Queue,
};

pub struct DynamicBufferDescriptor<'a> {
    pub label: Option<&'a str>,
    pub usage: BufferUsages,
}

pub struct DynamicBuffer {
    buffer: Buffer,
    size: BufferAddress,
    label: Option<String>,
    contents: Vec<u8>,
}
impl DynamicBuffer {
    pub fn new(device: &Device, descriptor: &DynamicBufferDescriptor) -> Self {
        let size = 4096;
        DynamicBuffer {
            buffer: device.create_buffer(&BufferDescriptor {
                label: descriptor.label, size, usage: descriptor.usage, mapped_at_creation: false,
            }),
            size, label: descriptor.label.map(|s| s.to_string()), contents: vec![],
        }
    }

    pub fn write_buffer(&mut self, device: &Device, queue: &Queue, contents: &[u8]) {
        if self.contents == contents {return}
        let (mut start, mut end) = (0, contents.len());
        if self.contents.len() == contents.len() && contents.len() >= 1024 {
            let prefix = contents.chunks_exact(64).zip(self.contents.chunks_exact(64))
                .take_while(|(a, b)| a == b).count() * 64;
            start = prefix + contents[prefix..].iter().zip(&self.contents[prefix..])
                .position(|(a, b)| a != b).unwrap();
            let suffix = contents[start..].rchunks_exact(64)
                .zip(self.contents[start..].rchunks_exact(64))
                .take_while(|(a, b)| a == b).count() * 64;
            end -= suffix;
            end = start + contents[start..end].iter().zip(&self.contents[start..end])
                .rposition(|(a, b)| a != b).unwrap() + 1;
            start &= !3;
            end = ((end + 3) & !3).min(contents.len());
            if end - start >= contents.len() - contents.len() / 4 {
                (start, end) = (0, contents.len());
            }
        }
        let pad = (4 - (end - start) % 4) % 4;
        let data = &contents[start..end];
        let data = if pad != 0 {&[data, &vec![0u8; pad]].concat()} else {data};

        if self.size >= (start + data.len()) as u64 {
            queue.write_buffer(&self.buffer, start as u64, data);
        } else {
            let mask = COPY_BUFFER_ALIGNMENT - 1;
            let size = ((data.len() as u64).next_power_of_two() + mask) & !mask;
            self.buffer = device.create_buffer(&BufferDescriptor {
                label: self.label.as_deref(), size, usage: self.buffer.usage(),
                mapped_at_creation: true,
            });
            self.buffer.slice(..).get_mapped_range_mut()[..data.len()].copy_from_slice(data);
            self.buffer.unmap();
            self.size = size;
        }
        if self.contents.len() == contents.len() {
            self.contents[start..end].copy_from_slice(&contents[start..end]);
        } else {
            self.contents.clear();
            self.contents.extend_from_slice(contents);
        }
    }

}
impl AsRef<Buffer> for DynamicBuffer {
    fn as_ref(&self) -> &Buffer {&self.buffer}
}

pub struct QuadIndexBuffer {
    buffer: DynamicBuffer,
    quads: u32,
}
impl QuadIndexBuffer {
    pub fn new(device: &Device) -> Self {
        Self {
            buffer: DynamicBuffer::new(device, &DynamicBufferDescriptor {
                label: None, usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            }),
            quads: 0,
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, quads: u32) {
        if quads <= self.quads {return}
        let quads = quads.next_power_of_two();
        let indices: Vec<_> = (0..quads).flat_map(|quad| {
            let vertex = quad * 4;
            [vertex, vertex+1, vertex+2, vertex+1, vertex+2, vertex+3]
        }).collect();
        self.buffer.write_buffer(device, queue, bytemuck::cast_slice(&indices));
        self.quads = quads;
    }
}
impl AsRef<Buffer> for QuadIndexBuffer {
    fn as_ref(&self) -> &Buffer {self.buffer.as_ref()}
}

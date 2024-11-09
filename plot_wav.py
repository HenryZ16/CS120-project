import matplotlib.pyplot as plt
from scipy.io import wavfile

# 读取WAV文件
sample_rate, data = wavfile.read('record.wav')

# 打印采样率和数据长度
print(f'Sample Rate: {sample_rate}')
print(f'Data Length: {len(data)}')

# 创建时间轴
time = [i / sample_rate for i in range(len(data))]

# 绘制音频波形
plt.figure(figsize=(10, 4))
plt.plot(time, data, label='Audio Signal')
plt.xlabel('Time [s]')
plt.ylabel('Amplitude')
plt.title('Audio Waveform')
plt.legend()
plt.grid()
plt.show()
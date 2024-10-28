import numpy as np
import matplotlib.pyplot as plt
from scipy.io import wavfile

# 读取 WAV 文件
sample_rate, data = wavfile.read('record.wav')

# 如果是立体声，选择一个通道
if len(data.shape) > 1:
    data = data[:, 0]

# 生成时间轴
time = np.linspace(0, len(data) / sample_rate, num=len(data))

# 绘制波形图
plt.figure(figsize=(10, 4))
plt.plot(time, data)
plt.title('Waveform of test.wav')
plt.xlabel('Time [s]')
plt.ylabel('Amplitude')
plt.grid()
plt.show()
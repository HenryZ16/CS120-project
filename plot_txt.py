import numpy as np
import matplotlib.pyplot as plt

def read_floats_from_file(file_path):
    with open(file_path, 'r') as file:
        lines = file.readlines()
        floats = [float(line.strip()) for line in lines]
    return np.array(floats)

# 读取文件并存储到 NumPy 数组
file_path = 'wav_data.txt'
data = read_floats_from_file(file_path)

# 打印 NumPy 数组
# print(data)

x = range(len(data))
plt.plot(x, data)
plt.show()
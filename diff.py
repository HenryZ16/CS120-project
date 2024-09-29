def compare_files(file1_path, file2_path):
    with open(file1_path, 'r') as file1, open(file2_path, 'r') as file2:
        content1 = file1.read()
        content2 = file2.read()

    # 确保两个文件的长度相同
    if len(content1) != len(content2):
        raise ValueError("Files have different lengths")

    # 统计不同字符的数量
    diff_count = sum(1 for a, b in zip(content1, content2) if a != b)

    return diff_count

def main():
    file1_path = 'output.txt'
    file2_path = 'testset/data.txt'
    try:
        diff_count = compare_files(file1_path, file2_path)
        print(f"Number of different characters: {diff_count}")
    except ValueError as e:
        print(e)

if __name__ == "__main__":
    main()
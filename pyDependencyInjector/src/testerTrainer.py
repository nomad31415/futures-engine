# exploring_name_main.py
import random
print(__name__)
print(random.__name__)
number = random.randint(1, 10)
if __name__ == "__main__":
    print("This script is in the top-level code environment")
    print(f"The number is {number}")

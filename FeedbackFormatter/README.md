# Feedback Formatter

This is a simple python program to format my feedback to student's PAs, Quizzes, Labs, etc.

### Requirements

- Python 3.*
- Computer

### Usage

- Create `feedback.csv` on your Desktop, and fill out as necessary
    - Use the following format: `subsection,points,points possible,description,notes`
    - `subsection` is a 0 or 1
    - `points` are points they scored
    - `points possible` are points possible for that main/sub- section
    - `description` is what the points are for; what you're scoring them on in that section
    - `notes` are notes. Write whatever, or nothing!
- Run `python feedbackformatter.py` in your terminal of choice.
    - Answer the prompt to give overall feedback at the end of the feedback notes
- Check out `pa#_feedback.txt` on your Desktop to see results!
- NOTE: to change PA #, edit `PA_NUMBER` at the top of the python file. Yes this is hardcoded. Deal with it.

### Example files

Here is an example csv input file:

```csv
subsection,points,points possible,description,notes
0,4,5,"Top-down design, style, & commenting",Nice comments!
0,17,18,Design and implementation of DietPlan class,
1,3,3,"Declaring goal calories, plan name, and date member variables",
1,2,2,Constructor,
1,2,2,Copy constructor,
1,0,1,Destructor,Missing destructor
1,10,15,Setter and getters,
1,4,4,editGoal function,
1,2,2,Others?,Nice job implementing overloaded operator>> and operator<<!
```

```txt
+--[ pts ]--+--[ pts possible ]--+------------------------[ Description ]------------------------+----------------------------------------[ Notes ]----------------------------------------+
|    4      |       5            | Top-down design, style, & commenting                          | Nice comments!                                                                          |
+-----------+--------------------+---------------------------------------------------------------+-----------------------------------------------------------------------------------------+
|   17      |      18            | Design and implementation of DietPlan class                   |                                                                                         |
|      3    |          3         | Declaring goal calories, plan name, and date member variables |                                                                                         |
|      2    |          2         | Constructor                                                   |                                                                                         |
|      2    |          2         | Copy constructor                                              |                                                                                         |
|      0    |          1         | Destructor                                                    | Missing destructor                                                                      |
|      10   |          15        | Setter and getters                                            |                                                                                         |
|      4    |          4         | editGoal function                                             |                                                                                         |
|      2    |          2         | Others?                                                       | Nice job implementing overloaded operator>> and operator<<! You fhbkasghfsjdukfghsgeukj |
+-----------+--------------------+---------------------------------------------------------------+-----------------------------------------------------------------------------------------+
| Total: 21/23                                                                                                                                                                             |
+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+
| Nice job on this PA~ Keep up the great work.                                                                                                                                             |
+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+
```
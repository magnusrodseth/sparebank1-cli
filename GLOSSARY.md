# SpareBank 1 banking

Terms for banking access, financial records, and views across selected profiles.

## Access

**Profile**:
A locally named banking login with its own authentication credentials.
_Avoid_: Person, bank account

**Default profile**:
The profile used for a banking read when the user makes no explicit profile selection.

**Profile selection**:
The profiles deliberately chosen for a banking read. Selecting every configured profile is one possible selection.

**Accessible account**:
A bank account that the authenticated user of a profile is permitted to view. Access alone does not establish ownership.
_Avoid_: Owned account, when only access has been established

## Financial records

**Account type**:
A bank's classification of an account, such as a credit card, BSU, ASK, or pension account.

**Booked transaction**:
A transaction that the bank has posted to an account.

**Bank classification**:
The bank's category and recurring or subscription attributes for a transaction.
_Avoid_: User annotation

**Personal finance context**:
Private explanations and preferences that give meaning to one profile's financial records, such as account purposes or expense-sharing arrangements.
_Avoid_: Bank classification

## Combined views

**Combined view**:
Financial information read together from a profile selection, with the source profiles identified.
_Avoid_: Household view, when household membership has not been established

**Shared account**:
One bank account accessible through more than one selected profile.
_Avoid_: Jointly owned account, when only access has been established

**Combined account summary**:
A financial overview of accessible accounts in a profile selection, with the source profiles identified.
_Avoid_: Household net worth, personal net worth

**Transfer between included accounts**:
A movement of money between accounts included in the same summary. It contributes neither income nor spending to that summary.
_Avoid_: Transfer between owned accounts, when only access has been established

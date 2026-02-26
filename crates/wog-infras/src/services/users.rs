use crate::repos::UserRepository;



struct UserServices<U: UserRepository> {
    user_repo: U,
    
}